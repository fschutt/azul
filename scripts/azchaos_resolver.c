/*
 * A resolver for one test domain, loaded into the app under test (DYLD_INSERT_LIBRARIES on
 * macOS, LD_PRELOAD on Linux). getaddrinfo asks the DNS server at $AZCHAOS_DNS_SERVER
 * ("ip:port", over UDP) for every name that ends in $AZCHAOS_DNS_SUFFIX, and the system's
 * resolver for every other name. It stands in for /etc/resolver/<domain> (macOS) or
 * /etc/resolv.conf (Linux), which only root may change, so a chaos test can point one
 * process at its own slow or silent DNS server (azchaos.SlowDns).
 *
 * Only A records are asked; the answer is one entry per address with the caller's socket
 * type. A name the server does not know is EAI_NONAME, a server that never answers is
 * EAI_AGAIN after AZCHAOS_DNS_WAIT seconds (default 30). The entries this file makes are
 * freed by its own freeaddrinfo: their layout is not the C library's.
 */

#define _GNU_SOURCE
#include <arpa/inet.h>
#include <errno.h>
#include <netdb.h>
#include <netinet/in.h>
#include <poll.h>
#include <pthread.h>
#include <stdint.h>
#include <stdlib.h>
#include <string.h>
#include <strings.h>
#include <sys/socket.h>
#include <sys/time.h>
#include <unistd.h>
#ifndef __APPLE__
#include <dlfcn.h>
#endif

/* One entry this file made: the entry and its address in one allocation. */
struct made {
    struct addrinfo info;
    struct sockaddr_in addr;
};

/* The lists this file handed out and not yet freed (their first entries). */
#define MAX_OUTSTANDING 4096
static struct addrinfo *outstanding[MAX_OUTSTANDING];
static pthread_mutex_t outstanding_lock = PTHREAD_MUTEX_INITIALIZER;

#ifdef __APPLE__
/* Inside the interposing image the names still mean the system's functions. */
static int real_getaddrinfo(const char *node, const char *service, const struct addrinfo *hints,
                            struct addrinfo **res) {
    return getaddrinfo(node, service, hints, res);
}
static void real_freeaddrinfo(struct addrinfo *list) { freeaddrinfo(list); }
#else
static int real_getaddrinfo(const char *node, const char *service, const struct addrinfo *hints,
                            struct addrinfo **res) {
    static int (*next)(const char *, const char *, const struct addrinfo *, struct addrinfo **);
    if (!next) {
        next = (int (*)(const char *, const char *, const struct addrinfo *,
                        struct addrinfo **))dlsym(RTLD_NEXT, "getaddrinfo");
    }
    return next(node, service, hints, res);
}
static void real_freeaddrinfo(struct addrinfo *list) {
    static void (*next)(struct addrinfo *);
    if (!next) {
        next = (void (*)(struct addrinfo *))dlsym(RTLD_NEXT, "freeaddrinfo");
    }
    next(list);
}
#endif

/* Whether `name` is one of the test domain's (case-blind, a trailing dot allowed). */
static int is_test_name(const char *name) {
    const char *suffix = getenv("AZCHAOS_DNS_SUFFIX");
    if (!name || !suffix || !*suffix) {
        return 0;
    }
    size_t n = strlen(name);
    if (n > 0 && name[n - 1] == '.') {
        n--;
    }
    size_t s = strlen(suffix);
    if (s > n) {
        return 0;
    }
    if (strncasecmp(name + n - s, suffix, s) != 0) {
        return 0;
    }
    return s == n || name[n - s - 1] == '.' || suffix[0] == '.';
}

/* The query for `name`'s A record into `out` (512 bytes); its length, or -1. */
static int build_query(const char *name, uint16_t id, unsigned char *out) {
    unsigned char *p = out;
    memset(p, 0, 12);
    p[0] = (unsigned char)(id >> 8);
    p[1] = (unsigned char)id;
    p[2] = 0x01; /* recursion desired */
    p[5] = 1;    /* one question */
    p += 12;
    const char *label = name;
    while (*label) {
        const char *dot = strchr(label, '.');
        size_t len = dot ? (size_t)(dot - label) : strlen(label);
        if (len == 0 || len > 63 || (size_t)(p - out) + len + 6 > 512) {
            if (len == 0 && dot && dot[1] == '\0') {
                break; /* the trailing dot */
            }
            return -1;
        }
        *p++ = (unsigned char)len;
        memcpy(p, label, len);
        p += len;
        if (!dot) {
            break;
        }
        label = dot + 1;
    }
    *p++ = 0;
    *p++ = 0;
    *p++ = 1; /* A */
    *p++ = 0;
    *p++ = 1; /* IN */
    return (int)(p - out);
}

/* The offset after the name at `at` of `msg`, or -1. */
static int skip_name(const unsigned char *msg, int len, int at) {
    while (at < len) {
        unsigned char c = msg[at];
        if (c == 0) {
            return at + 1;
        }
        if ((c & 0xc0) == 0xc0) {
            return at + 2 <= len ? at + 2 : -1;
        }
        at += 1 + c;
    }
    return -1;
}

/* Asks the test's server for the IPv4 addresses of `name`: 0 with them in `found` (at most
 * `max`, how many in `count`), or an EAI_ code (positive on macOS, negative on Linux). */
static int ask(const char *name, struct in_addr *found, int max, int *count) {
    const char *server = getenv("AZCHAOS_DNS_SERVER");
    if (!server) {
        return EAI_FAIL;
    }
    char host[64];
    const char *colon = strrchr(server, ':');
    if (!colon || (size_t)(colon - server) >= sizeof host) {
        return EAI_FAIL;
    }
    memcpy(host, server, (size_t)(colon - server));
    host[colon - server] = '\0';
    struct sockaddr_in to;
    memset(&to, 0, sizeof to);
    to.sin_family = AF_INET;
    to.sin_port = htons((uint16_t)atoi(colon + 1));
    if (inet_pton(AF_INET, host, &to.sin_addr) != 1) {
        return EAI_FAIL;
    }
    unsigned char query[512];
    struct timeval now;
    gettimeofday(&now, NULL);
    uint16_t id = (uint16_t)(now.tv_usec ^ (uintptr_t)&now);
    int qlen = build_query(name, id, query);
    if (qlen < 0) {
        return EAI_NONAME;
    }
    int sock = socket(AF_INET, SOCK_DGRAM, 0);
    if (sock < 0) {
        return EAI_SYSTEM;
    }
    if (sendto(sock, query, (size_t)qlen, 0, (struct sockaddr *)&to, sizeof to) != qlen) {
        close(sock);
        return EAI_AGAIN;
    }
    const char *wait_text = getenv("AZCHAOS_DNS_WAIT");
    int wait_ms = (wait_text ? atoi(wait_text) : 30) * 1000;
    unsigned char answer[1500];
    int len = -1;
    for (;;) {
        struct pollfd pfd = {sock, POLLIN, 0};
        int ready = poll(&pfd, 1, wait_ms);
        if (ready < 0 && errno == EINTR) {
            continue;
        }
        if (ready <= 0) {
            break;
        }
        len = (int)recv(sock, answer, sizeof answer, 0);
        if (len >= 12 && answer[0] == query[0] && answer[1] == query[1]) {
            break;
        }
        len = -1;
    }
    close(sock);
    if (len < 12) {
        return EAI_AGAIN;
    }
    if ((answer[3] & 0x0f) == 3) {
        return EAI_NONAME;
    }
    int questions = (answer[4] << 8) | answer[5];
    int answers = (answer[6] << 8) | answer[7];
    int at = 12;
    for (int i = 0; i < questions && at >= 0; i++) {
        at = skip_name(answer, len, at);
        at = at >= 0 ? at + 4 : -1;
    }
    *count = 0;
    for (int i = 0; i < answers && at >= 0 && *count < max; i++) {
        at = skip_name(answer, len, at);
        if (at < 0 || at + 10 > len) {
            break;
        }
        int type = (answer[at] << 8) | answer[at + 1];
        int rdlength = (answer[at + 8] << 8) | answer[at + 9];
        at += 10;
        if (at + rdlength > len) {
            break;
        }
        if (type == 1 && rdlength == 4) {
            memcpy(&found[(*count)++], answer + at, 4);
        }
        at += rdlength;
    }
    return *count > 0 ? 0 : EAI_NONAME;
}

static int test_getaddrinfo(const char *node, const char *service, const struct addrinfo *hints,
                            struct addrinfo **res) {
    if (!is_test_name(node)) {
        return real_getaddrinfo(node, service, hints, res);
    }
    if (hints && hints->ai_family != AF_UNSPEC && hints->ai_family != AF_INET) {
        return EAI_NONAME;
    }
    uint16_t port = 0;
    if (service) {
        char *end = NULL;
        long number = strtol(service, &end, 10);
        if (!*service || *end || number < 0 || number > 65535) {
            return EAI_SERVICE;
        }
        port = (uint16_t)number;
    }
    struct in_addr found[16];
    int count = 0;
    int asked = ask(node, found, 16, &count);
    if (asked != 0) {
        return asked;
    }
    int socktype = hints && hints->ai_socktype ? hints->ai_socktype : SOCK_STREAM;
    int protocol = hints && hints->ai_protocol ? hints->ai_protocol
                   : socktype == SOCK_DGRAM    ? IPPROTO_UDP
                                               : IPPROTO_TCP;
    struct addrinfo *first = NULL;
    struct addrinfo **link = &first;
    for (int i = 0; i < count; i++) {
        struct made *m = calloc(1, sizeof *m);
        if (!m) {
            break;
        }
        m->addr.sin_family = AF_INET;
#ifdef __APPLE__
        m->addr.sin_len = sizeof m->addr;
#endif
        m->addr.sin_port = htons(port);
        m->addr.sin_addr = found[i];
        m->info.ai_family = AF_INET;
        m->info.ai_socktype = socktype;
        m->info.ai_protocol = protocol;
        m->info.ai_addrlen = sizeof m->addr;
        m->info.ai_addr = (struct sockaddr *)&m->addr;
        *link = &m->info;
        link = &m->info.ai_next;
    }
    if (!first) {
        return EAI_MEMORY;
    }
    if (hints && (hints->ai_flags & AI_CANONNAME)) {
        first->ai_canonname = strdup(node);
    }
    pthread_mutex_lock(&outstanding_lock);
    for (int i = 0; i < MAX_OUTSTANDING; i++) {
        if (!outstanding[i]) {
            outstanding[i] = first;
            break;
        }
    }
    pthread_mutex_unlock(&outstanding_lock);
    *res = first;
    return 0;
}

static void test_freeaddrinfo(struct addrinfo *list) {
    int ours = 0;
    pthread_mutex_lock(&outstanding_lock);
    for (int i = 0; list && i < MAX_OUTSTANDING; i++) {
        if (outstanding[i] == list) {
            outstanding[i] = NULL;
            ours = 1;
            break;
        }
    }
    pthread_mutex_unlock(&outstanding_lock);
    if (!ours) {
        real_freeaddrinfo(list);
        return;
    }
    free(list->ai_canonname);
    while (list) {
        struct addrinfo *next = list->ai_next;
        free(list);
        list = next;
    }
}

#ifdef __APPLE__
/* dyld's interposing: every image's calls of the second go to the first. */
__attribute__((used)) static const struct {
    const void *replacement;
    const void *replacee;
} interposers[] __attribute__((section("__DATA,__interpose"))) = {
    {(const void *)test_getaddrinfo, (const void *)getaddrinfo},
    {(const void *)test_freeaddrinfo, (const void *)freeaddrinfo},
};
#else
int getaddrinfo(const char *node, const char *service, const struct addrinfo *hints,
                struct addrinfo **res) {
    return test_getaddrinfo(node, service, hints, res);
}
void freeaddrinfo(struct addrinfo *list) { test_freeaddrinfo(list); }
#endif
