// mechb harness — native aarch64 executor for remill-lifted join_generic_copy
// Input model: <[&str]>::join on ["5"] with sep " "
//   X0=slice ptr, X1=1, X2=sep ptr, X3=1, X8=sret(24B Vec{cap,ptr,len})
// CORRECT result: {cap>=1, ptr=heap, len=1}, heap[0]=='5'
// BUG signature (mechB): {ptr==1, len==P(element ptr)} + do_reserve(additional==P)
#include <stdio.h>
#include <stdint.h>
#include <string.h>
#include <stdlib.h>

extern "C" {
void* sub_b3dd90(void*, uint64_t, void*);
uint64_t off_X0(); uint64_t off_X1(); uint64_t off_X2(); uint64_t off_X3();
uint64_t off_X4(); uint64_t off_X5(); uint64_t off_X6(); uint64_t off_X7();
uint64_t off_X8(); uint64_t off_X9(); uint64_t off_X10(); uint64_t off_X11();
uint64_t off_X12(); uint64_t off_X13(); uint64_t off_X14(); uint64_t off_X15();
uint64_t off_X16(); uint64_t off_X17(); uint64_t off_X18(); uint64_t off_X19();
uint64_t off_X20(); uint64_t off_X21(); uint64_t off_X22(); uint64_t off_X23();
uint64_t off_X24(); uint64_t off_X25(); uint64_t off_X26(); uint64_t off_X27();
uint64_t off_X28(); uint64_t off_X29(); uint64_t off_X30();
uint64_t off_SP(); uint64_t off_PC();
}

static uint8_t STATE[32768];
static uint64_t OX[31], OSP, OPC;
static inline uint64_t& X(int n) { return *(uint64_t*)(STATE + OX[n]); }
static inline uint64_t& SPr()    { return *(uint64_t*)(STATE + OSP); }

// guest memory = identity host pointers
static uint8_t  GSTACK[1 << 20];
static uint8_t  BUMP[64 << 20];
static size_t   bump_off = 0;
static uint8_t  ELEM[16] = { '5' };   // "P" — the element's bytes
static uint64_t SLICE[2];             // the &[&str]: { ELEM, 1 }
static uint8_t  SEP[16]  = { ' ' };
static uint8_t  RETBUF[64];
static uint64_t SP0;

static int trace_on = 1;
#define TR(...) do { if (trace_on) fprintf(stderr, __VA_ARGS__); } while (0)

static const char* ann(uint64_t a, char* b) {
  uint64_t s = (uint64_t)SLICE, e = (uint64_t)ELEM, p = (uint64_t)SEP,
           r = (uint64_t)RETBUF, g = (uint64_t)GSTACK, h = (uint64_t)BUMP;
  if (a >= s && a < s + 16)              sprintf(b, "SLICE+%llx", a - s);
  else if (a >= e && a < e + 16)         sprintf(b, "P+%llx", a - e);
  else if (a >= p && a < p + 16)         sprintf(b, "SEP+%llx", a - p);
  else if (a >= r && a < r + 64)         sprintf(b, "RET+%llx", a - r);
  else if (a >= g && a < g + sizeof(GSTACK)) sprintf(b, "STK-%llx", SP0 - a);
  else if (a >= h && a < h + sizeof(BUMP))   sprintf(b, "HEAP+%llx", a - h);
  else                                   sprintf(b, "%llx", a);
  return b;
}

extern "C" {
uint64_t __remill_read_memory_64(void* m, uint64_t a) {
  uint64_t v = *(uint64_t*)a; char b1[64], b2[64];
  TR("  R64 [%s] -> %s\n", ann(a, b1), ann(v, b2)); return v;
}
uint32_t __remill_read_memory_32(void* m, uint64_t a) {
  uint32_t v = *(uint32_t*)a; char b1[64];
  TR("  R32 [%s] -> %x\n", ann(a, b1), v); return v;
}
uint16_t __remill_read_memory_16(void* m, uint64_t a) {
  uint16_t v = *(uint16_t*)a; char b1[64];
  TR("  R16 [%s] -> %x\n", ann(a, b1), v); return v;
}
uint8_t __remill_read_memory_8(void* m, uint64_t a) {
  uint8_t v = *(uint8_t*)a; char b1[64];
  TR("  R8  [%s] -> %x\n", ann(a, b1), v); return v;
}
void* __remill_write_memory_64(void* m, uint64_t a, uint64_t v) {
  char b1[64], b2[64]; TR("  W64 [%s] <- %s\n", ann(a, b1), ann(v, b2));
  *(uint64_t*)a = v; return m;
}
void* __remill_write_memory_32(void* m, uint64_t a, uint32_t v) {
  char b1[64]; TR("  W32 [%s] <- %x\n", ann(a, b1), v);
  *(uint32_t*)a = v; return m;
}
void* __remill_write_memory_16(void* m, uint64_t a, uint16_t v) {
  char b1[64]; TR("  W16 [%s] <- %x\n", ann(a, b1), v);
  *(uint16_t*)a = v; return m;
}
void* __remill_write_memory_8(void* m, uint64_t a, uint8_t v) {
  char b1[64]; TR("  W8  [%s] <- %x\n", ann(a, b1), v);
  *(uint8_t*)a = v; return m;
}
bool __remill_flag_computation_sign(bool r, ...)     { return r; }
bool __remill_flag_computation_zero(bool r, ...)     { return r; }
bool __remill_flag_computation_overflow(bool r, ...) { return r; }
bool __remill_flag_computation_carry(bool r, ...)    { return r; }
bool __remill_compare_eq(bool r)  { return r; }
bool __remill_compare_neq(bool r) { return r; }
bool __remill_compare_sle(bool r) { return r; }
bool __remill_compare_slt(bool r) { return r; }
bool __remill_compare_sge(bool r) { return r; }
bool __remill_compare_sgt(bool r) { return r; }
bool __remill_compare_ugt(bool r) { return r; }
bool __remill_compare_uge(bool r) { return r; }
bool __remill_compare_ule(bool r) { return r; }
bool __remill_compare_ult(bool r) { return r; }
void* __remill_function_return(void* s, uint64_t pc, void* m) {
  TR("  [ret to %llx]\n", pc); return m;
}
void* __remill_missing_block(void* s, uint64_t pc, void* m) {
  TR("  !! MISSING_BLOCK pc=%llx\n", pc); return m;
}
void* __remill_error(void* s, uint64_t pc, void* m) {
  TR("  !! REMILL_ERROR pc=%llx\n", pc); exit(9);
}

// ---- callees ----
void* sub_3af378(void* s, uint64_t pc, void* m) {  // no_alloc_shim_is_unstable
  TR("CALL no_alloc_shim\n"); return m;
}
void* sub_3af35c(void* s, uint64_t pc, void* m) {  // __rust_alloc(size, align)
  uint64_t size = X(0), align = X(1);
  bump_off = (bump_off + 15) & ~15ULL;
  if (bump_off + size > sizeof(BUMP)) {
    TR("CALL __rust_alloc(size=%llx) -> OOM (insane size?)\n", size);
    X(0) = 0; return m;
  }
  uint64_t p = (uint64_t)(BUMP + bump_off); bump_off += size;
  TR("CALL __rust_alloc(size=%llx, align=%llx) -> HEAP+%llx\n", size, align,
     p - (uint64_t)BUMP);
  X(0) = p; return m;
}
static void* memcpy_island(void* s, uint64_t pc, void* m) {
  uint64_t d = X(0), src = X(1), n = X(2); char b1[64], b2[64];
  TR("CALL memcpy(dst=%s, src=%s, n=%llx)\n", ann(d, b1), ann(src, b2), n);
  if (n <= sizeof(BUMP)) memcpy((void*)d, (void*)src, n);
  else TR("  !! insane memcpy size — skipped (BUG SIGNATURE)\n");
  return m;
}
void* sub_3fdc9d8(void* s, uint64_t pc, void* m) { return memcpy_island(s, pc, m); }
#define ISLAND(N) \
  void* island##N(void*, uint64_t, void*) __asm__("_sub_3fdc9d8." #N); \
  void* island##N(void* s, uint64_t pc, void* m) { return memcpy_island(s, pc, m); }
ISLAND(1) ISLAND(2) ISLAND(3) ISLAND(4) ISLAND(5) ISLAND(6) ISLAND(7)

void* sub_3b55578(void* s, uint64_t pc, void* m) {
  // RawVecInner::reserve::do_reserve_and_handle(&mut inner, len, additional, layout)
  uint64_t inner = X(0), len = X(1), addl = X(2);
  uint64_t* cap = (uint64_t*)inner;
  uint64_t* ptr = (uint64_t*)(inner + 8);
  char b1[64], b2[64];
  TR("CALL do_reserve(inner=%s {cap=%llx, ptr=%s}, len=%llx, additional=%llx)\n",
     ann(inner, b1), *cap, ann(*ptr, b2), len, addl);
  if (addl > (32ULL << 20))
    TR("  !! INSANE additional — mechB signature (element PTR used as a length)\n");
  uint64_t need = len + addl;
  uint64_t newcap = (*cap * 2 > need) ? *cap * 2 : need;
  if (newcap < 8) newcap = 8;
  bump_off = (bump_off + 15) & ~15ULL;
  if (bump_off + newcap > sizeof(BUMP)) {
    TR("  -> OOM, inner left unchanged\n"); return m;
  }
  uint64_t np = (uint64_t)(BUMP + bump_off); bump_off += newcap;
  if (len && *ptr > 1) memcpy((void*)np, (void*)*ptr, len);
  *cap = newcap; *ptr = np;
  TR("  -> {cap=%llx, ptr=HEAP+%llx}\n", *cap, np - (uint64_t)BUMP);
  return m;
}
void* sub_3b7cd20(void* s, uint64_t pc, void* m) {
  TR("PANIC: option::expect_failed (len overflow precompute)\n"); exit(3);
}
void* sub_3b7cbb4(void* s, uint64_t pc, void* m) {
  TR("PANIC: raw_vec::handle_error (cap overflow / alloc fail)\n"); exit(4);
}
void* sub_3b7cd68(void* s, uint64_t pc, void* m) {
  TR("PANIC: core::panicking::panic_fmt (slice-index fail path)\n"); exit(5);
}
}

int main(int argc, char** argv) {
  if (argc > 1 && !strcmp(argv[1], "-q")) trace_on = 0;
  uint64_t (*offs[])() = { off_X0, off_X1, off_X2, off_X3, off_X4, off_X5,
    off_X6, off_X7, off_X8, off_X9, off_X10, off_X11, off_X12, off_X13,
    off_X14, off_X15, off_X16, off_X17, off_X18, off_X19, off_X20, off_X21,
    off_X22, off_X23, off_X24, off_X25, off_X26, off_X27, off_X28, off_X29,
    off_X30 };
  for (int i = 0; i < 31; i++) OX[i] = offs[i]();
  OSP = off_SP(); OPC = off_PC();
  fprintf(stderr, "offsets: X0=%llu X8=%llu X26=%llu SP=%llu PC=%llu\n",
          OX[0], OX[8], OX[26], OSP, OPC);
  if (OPC + 8 > sizeof(STATE)) { fprintf(stderr, "STATE buf too small\n"); return 7; }

  memset(STATE, 0, sizeof STATE);
  SLICE[0] = (uint64_t)ELEM; SLICE[1] = 1;
  SP0 = (uint64_t)(GSTACK + sizeof(GSTACK) - 256);
  X(0) = (uint64_t)SLICE; X(1) = 1;
  X(2) = (uint64_t)SEP;   X(3) = 1;
  X(8) = (uint64_t)RETBUF;
  X(30) = 0xDEAD0000DEAD0000ULL;
  SPr() = SP0;
  *(uint64_t*)(STATE + OPC) = 0xb3dd90;

  char b1[64];
  fprintf(stderr, "INPUT: slice=%llx elem P=%llx ('5',len 1) sep=%llx ret=%llx sp=%llx\n",
          (uint64_t)SLICE, (uint64_t)ELEM, (uint64_t)SEP, (uint64_t)RETBUF, SP0);
  sub_b3dd90(STATE, 0xb3dd90, (void*)0x1234);

  uint64_t rcap = *(uint64_t*)RETBUF, rptr = *(uint64_t*)(RETBUF + 8),
           rlen = *(uint64_t*)(RETBUF + 16);
  printf("RESULT: cap=%llx ptr=%s len=%llx\n", rcap, ann(rptr, b1), rlen);
  int ok = (rlen == 1) && rptr > 1 && (*(char*)rptr == '5');
  if (rlen == (uint64_t)ELEM) printf("  >> len == element PTR P — THE mechB corruption\n");
  if (rptr == 1)              printf("  >> ptr == 1 (dangling) — vec ptr never updated\n");
  printf(ok ? "VERDICT: CORRECT\n" : "VERDICT: *** CORRUPT ***\n");
  return ok ? 0 : 1;
}
