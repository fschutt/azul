#!/usr/bin/env python3
"""A mock mail account on the Azlin stack: signs up a test drive and fills its mailbox.

The mailbox is files in the drive (examples/azul-mail/AZLIN_MAIL.md): every message one .eml
object under mail/<Folder>/, named <stamp>-<first 16 hex of SHA-256>.eml - what the customer's
Cloudflare Email Worker will write later. This script is that Worker's stand-in:

1. POST <token server>/v1/drives: a new drive (a development token server's sign-up);
2. puts realistic, made-up mail into it with the bundle's S3 credentials (SigV4, path style):
   into mail/Inbox/ a plain-text mail with a quote, an HTML mail with an inline picture (cid:),
   a mail with an attachment and a calendar invitation (text/calendar; method=REQUEST); into
   mail/Spam/ one spam mail; with --big-mb N also one mail with an N MiB attachment (AzMail lists
   it from its header block and downloads it when it is opened);
3. lists mail/ again and prints what is where (never the drive token);
4. with --out FILE writes what the e2e needs - the token server, the drive id, the drive token,
   the bundle's credentials, the seeded keys and subjects - to FILE (mode 0600: it holds the
   drive token).

The token server: --token-url, else $AZLIN_TOKEN_URL, else the shared Azlin config's endpoints,
else http://127.0.0.1:8081 (azul-apps iso/: `azctl dev up --processes`; scripts/azlin_mock_stack.py
--token-port 8081 --s3-port 9000 is the Python stand-in). --s3-url reaches the bucket at another
address than the bundle's endpoint (http://127.0.0.1:9000 locally).

    python3 scripts/azmail_seed_azlin.py [--token-url URL] [--s3-url URL] [--out seed.json]
        [--big-mb 6] [--name "AzMail seed"]
"""
import argparse
import base64
import email.policy
import email.utils
import json
import os
import struct
import sys
import time
import zlib
from email.message import EmailMessage

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)

import azlin_client  # noqa: E402

# The account the mail is for, and the people it is from (all made up; example.org is reserved).
ME = ('Ada Lovelace', 'ada@example.org')
BEN = ('Ben Okafor', 'ben@example.org')
CLEO = ('Cleo Marsh', 'cleo@example.org')
SHOP = ('Greenfinger Seeds', 'orders@seeds.example.org')
SPAMMER = ('Prize Department', 'winner@prizes.example.net')

SUBJECTS = {
    'plain': 'Allotment rota for November',
    'html': 'Harvest festival poster',
    'attachment': 'Your seed order 2027',
    'invite': 'Invitation: Allotment committee meeting',
    'spam': 'You have won a greenhouse!!!',
    'big': 'Photos of the harvest festival',
}


def address(pair):
    return email.utils.formataddr(pair)


def png(width=16, height=16, rgb=(46, 125, 50)):
    """A small PNG of one colour (no imaging library needed)."""
    raw = b''.join(b'\x00' + bytes(rgb) * width for _ in range(height))

    def chunk(kind, data):
        return (struct.pack('>I', len(data)) + kind + data
                + struct.pack('>I', zlib.crc32(kind + data) & 0xFFFFFFFF))

    header = struct.pack('>IIBBBBB', width, height, 8, 2, 0, 0, 0)
    return (b'\x89PNG\r\n\x1a\n' + chunk(b'IHDR', header)
            + chunk(b'IDAT', zlib.compress(raw, 9)) + chunk(b'IEND', b''))


def base(subject, sender, sent, message_id, to=ME):
    msg = EmailMessage()
    msg['From'] = address(sender)
    msg['To'] = address(to)
    msg['Subject'] = subject
    msg['Date'] = email.utils.formatdate(sent, usegmt=True)
    msg['Message-ID'] = message_id
    return msg


def plain_mail(sent):
    msg = base(SUBJECTS['plain'], BEN, sent, '<seed-plain@azmail.example.org>')
    msg['In-Reply-To'] = '<rota-1@azmail.example.org>'
    msg['References'] = '<rota-1@azmail.example.org>'
    msg.set_content(
        'Hi Ada,\n\n'
        'here is the rota for November. I swapped the second Saturday with Cleo,\n'
        'she is away at her sister\'s.\n\n'
        '  Sat  2 Nov  Ben   - compost and the water butts\n'
        '  Sat  9 Nov  Ada   - leaf mould, cover the beds\n'
        '  Sat 16 Nov  Cleo  - garlic and the broad beans\n'
        '  Sat 23 Nov  Ben   - tool shed, oil the hinges\n\n'
        'Ada Lovelace wrote:\n'
        '> Can we sort out the rota before the frost comes?\n'
        '> I can do any Saturday but the first one.\n\n'
        'Cheers,\nBen\n')
    return msg


def html_mail(sent):
    msg = base(SUBJECTS['html'], CLEO, sent, '<seed-html@azmail.example.org>')
    msg.set_content('The harvest festival is on Saturday 12 October, 2 pm, at the allotment '
                    'gate. Bring a dish to share!\n')
    msg.add_alternative(
        '<!DOCTYPE html><html><body style="font-family: Georgia, serif; color: #1b3d1f">'
        '<h1 style="color: #2e7d32">Harvest festival</h1>'
        '<p><img src="cid:poster@seed.example.org" alt="A green poster" width="64" height="64"></p>'
        '<p>Saturday <b>12 October</b>, 2 pm, at the allotment gate.</p>'
        '<ul><li>Bring a dish to share</li><li>Prizes for the biggest marrow</li></ul>'
        '<p>See you there,<br>Cleo</p></body></html>', subtype='html')
    html_part = msg.get_payload()[1]
    html_part.add_related(png(), maintype='image', subtype='png', cid='<poster@seed.example.org>',
                          filename='poster.png')
    return msg


def attachment_mail(sent):
    msg = base(SUBJECTS['attachment'], SHOP, sent, '<seed-order@seeds.example.org>')
    msg.set_content('Dear Ada Lovelace,\n\nthank you for your order. Your seeds leave our '
                    'warehouse within three days; the order is attached as a spreadsheet.\n\n'
                    'Greenfinger Seeds\n')
    order = ('item,variety,packets,price\n'
             'tomato,Gardener\'s Delight,2,3.40\n'
             'bean,Cobra (climbing),1,2.95\n'
             'garlic,Elephant,3,8.85\n').encode('utf-8')
    msg.add_attachment(order, maintype='text', subtype='csv', filename='seed-order-2027.csv')
    return msg


def invite_mail(sent):
    msg = base(SUBJECTS['invite'], BEN, sent, '<seed-invite@azmail.example.org>')
    start = time.gmtime(sent + 6 * 86400)
    day = time.strftime('%Y%m%d', start)
    stamp = time.strftime('%Y%m%dT%H%M%SZ', time.gmtime(sent))
    ics = '\r\n'.join([
        'BEGIN:VCALENDAR',
        'VERSION:2.0',
        'PRODID:-//AzMail seed//EN',
        'METHOD:REQUEST',
        'BEGIN:VEVENT',
        'UID:seed-committee@azmail.example.org',
        'DTSTAMP:' + stamp,
        'DTSTART:%sT180000Z' % day,
        'DTEND:%sT193000Z' % day,
        'SUMMARY:Allotment committee meeting',
        'LOCATION:The shed by plot 7',
        'ORGANIZER;CN=Ben Okafor:mailto:ben@example.org',
        'ATTENDEE;CN=Ada Lovelace;RSVP=TRUE:mailto:ada@example.org',
        'END:VEVENT',
        'END:VCALENDAR',
        '',
    ])
    msg.set_content('Ben Okafor invites you to the allotment committee meeting, '
                    'in the shed by plot 7.\n')
    msg.add_alternative(ics, subtype='calendar', params={'method': 'REQUEST'})
    return msg


def spam_mail(sent):
    msg = base(SUBJECTS['spam'], SPAMMER, sent, '<seed-spam@prizes.example.net>')
    msg.set_content('CONGRATULATIONS!!! You have been selected to receive a FREE greenhouse. '
                    'Reply with your bank details to claim it.\n')
    return msg


def big_mail(sent, megabytes):
    msg = base(SUBJECTS['big'], CLEO, sent, '<seed-big@azmail.example.org>')
    msg.set_content('The photos from Saturday, all in one archive.\n')
    size = megabytes << 20
    pattern = b'AZMAIL-SEED-'
    data = (pattern * (size // len(pattern) + 1))[:size]
    msg.add_attachment(data, maintype='application', subtype='octet-stream',
                       filename='harvest-festival-photos.bin')
    return msg


def wire(msg):
    """The exact bytes of the message: RFC 5322 with CRLF line ends."""
    return msg.as_bytes(policy=email.policy.SMTP)


def seed(token_url, s3_url=None, name='AzMail seed', big_mb=0, now=None):
    """Signs up a drive at `token_url`, fills its mailbox; returns what --out writes."""
    client = azlin_client.TokenClient(token_url)
    status, bundle, text = client.signup(name)
    if status != 201 or not isinstance(bundle, dict):
        raise SystemExit('the sign-up at %s failed: HTTP %d %s' % (token_url, status, text[:300]))
    drive_id, bucket_name, endpoint, _ = azlin_client.bundle_drive(bundle)
    bucket = azlin_client.Bucket(bundle, endpoint=s3_url)
    now = int(time.time() if now is None else now)
    # (folder, kind, when it was sent, how it is made); the name's stamp is its date, as an
    # import's would be.
    mail = [
        ('Inbox', 'plain', now - 3 * 3600, plain_mail),
        ('Inbox', 'html', now - 2 * 3600, html_mail),
        ('Inbox', 'attachment', now - 26 * 3600, attachment_mail),
        ('Inbox', 'invite', now - 3600, invite_mail),
        ('Spam', 'spam', now - 5 * 3600, spam_mail),
    ]
    if big_mb > 0:
        mail.append(('Inbox', 'big', now - 600, lambda sent: big_mail(sent, big_mb)))
    seeded = []
    for folder, kind, sent, make in mail:
        msg = make(sent)
        data = wire(msg)
        key = azlin_client.message_key(folder, azlin_client.object_name(data, sent))
        bucket.put(key, data)
        seeded.append({'folder': folder, 'kind': kind, 'key': key, 'subject': msg['Subject'],
                       'size': len(data)})
    objects, folders = bucket.list('mail/', delimiter='/')
    listed = {key for folder in folders for key in bucket.keys(folder)}
    missing = [m['key'] for m in seeded if m['key'] not in listed]
    if missing:
        raise SystemExit('not in the bucket after the upload: %s' % missing)
    return {
        'token_url': token_url,
        's3_url': bucket.endpoint,
        'drive_id': drive_id,
        'bucket': bucket_name,
        'drive_token': bundle.get('drive_token') or '',
        'bundle': bundle,
        'messages': seeded,
        'folders': sorted(f[len('mail/'):].rstrip('/') for f in folders),
        'loose_objects': [key for key, _ in objects],
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__.split('\n\n')[0])
    parser.add_argument('--token-url')
    parser.add_argument('--s3-url', help="reach the bucket here instead of the bundle's endpoint")
    parser.add_argument('--name', default='AzMail seed', help="the drive's name")
    parser.add_argument('--big-mb', type=int, default=0,
                        help='also one mail with an attachment of this many MiB')
    parser.add_argument('--out', help='write the drive (with its token: mode 0600) and the '
                                      'seeded mail as JSON here')
    args = parser.parse_args()
    token_url = azlin_client.token_url_from(args.token_url)
    result = seed(token_url, args.s3_url, args.name, args.big_mb)
    print('drive %s (bucket %s at %s), token server %s' % (result['drive_id'], result['bucket'],
                                                          result['s3_url'], token_url))
    for m in result['messages']:
        print('  %-6s %8d bytes  %s  %s' % (m['folder'], m['size'], m['key'], m['subject']))
    if args.out:
        descriptor = os.open(args.out, os.O_WRONLY | os.O_CREAT | os.O_TRUNC, 0o600)
        with os.fdopen(descriptor, 'w', encoding='utf-8') as f:
            json.dump(result, f, indent=2)
            f.write('\n')
        print('written: %s (it holds the drive token)' % args.out)
    print('AZMAIL_SEED_DONE %s %d' % (result['drive_id'], len(result['messages'])), flush=True)


if __name__ == '__main__':
    main()
