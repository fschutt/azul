// wdis.mjs <wasm> <hexoff> [len] — linear-disassemble llc-emitted wasm bytes at a
// raw file offset. Not a full spec decoder: covers the opcode subset llc/wasm-ld
// actually emit in this pipeline; bails loudly on anything unknown.
import fs from 'fs';
const [file, offHex, lenHex] = process.argv.slice(2);
const buf = fs.readFileSync(file);
let p = parseInt(offHex, 16);
const end = p + (lenHex ? parseInt(lenHex, 16) : 0x200);
const u = () => { let r = 0n, s = 0n, b; do { b = buf[p++]; r |= BigInt(b & 0x7f) << s; s += 7n; } while (b & 0x80); return r; };
const sv = () => { let r = 0n, s = 0n, b; do { b = buf[p++]; r |= BigInt(b & 0x7f) << s; s += 7n; } while (b & 0x80); if (b & 0x40) r |= -1n << s; return r; };
const REG = { 2216:'RAX',2232:'RBX',2248:'RCX',2264:'RDX',2280:'RSI',2296:'RDI',2312:'RSP',2328:'RBP',2344:'R8',2360:'R9',2376:'R10',2392:'R11',2408:'R14',2424:'R15',2680:'PC' };
const mem = (n) => { const a = u(), o = u(); const r = REG[Number(o)]; return `${n} align=${a} offset=${o}${r ? ' <' + r + '>' : ''}`; };
const OPS = {
  0x00:()=>'unreachable',0x01:()=>'nop',0x02:()=>{const t=buf[p++];return `block ${t.toString(16)}`},0x03:()=>{const t=buf[p++];return `loop ${t.toString(16)}`},
  0x04:()=>{const t=buf[p++];return `if ${t.toString(16)}`},0x05:()=>'else',0x0b:()=>'end',
  0x0c:()=>`br ${u()}`,0x0d:()=>`br_if ${u()}`,
  0x0e:()=>{const n=Number(u());const t=[];for(let i=0;i<=n;i++)t.push(u());return `br_table [${t.join(',')}]`},
  0x0f:()=>'return',0x10:()=>`call func[${u()}]`,0x11:()=>{const t=u(),tb=u();return `call_indirect type=${t} table=${tb}`},
  0x1a:()=>'drop',0x1b:()=>'select',
  0x20:()=>`local.get ${u()}`,0x21:()=>`local.set ${u()}`,0x22:()=>`local.tee ${u()}`,0x23:()=>`global.get ${u()}`,0x24:()=>`global.set ${u()}`,
  0x28:()=>mem('i32.load'),0x29:()=>mem('i64.load'),0x2a:()=>mem('f32.load'),0x2b:()=>mem('f64.load'),
  0x2c:()=>mem('i32.load8_s'),0x2d:()=>mem('i32.load8_u'),0x2e:()=>mem('i32.load16_s'),0x2f:()=>mem('i32.load16_u'),
  0x30:()=>mem('i64.load8_s'),0x31:()=>mem('i64.load8_u'),0x32:()=>mem('i64.load16_s'),0x33:()=>mem('i64.load16_u'),
  0x34:()=>mem('i64.load32_s'),0x35:()=>mem('i64.load32_u'),
  0x36:()=>mem('i32.store'),0x37:()=>mem('i64.store'),0x38:()=>mem('f32.store'),0x39:()=>mem('f64.store'),
  0x3a:()=>mem('i32.store8'),0x3b:()=>mem('i32.store16'),0x3c:()=>mem('i64.store8'),0x3d:()=>mem('i64.store16'),0x3e:()=>mem('i64.store32'),
  0x3f:()=>{p++;return 'memory.size'},0x40:()=>{p++;return 'memory.grow'},
  0x41:()=>`i32.const ${sv()}`,0x42:()=>{const v=sv();return `i64.const ${v}${v>255n?' (0x'+v.toString(16)+')':''}`},
  0x43:()=>{const v=buf.readFloatLE(p);p+=4;return `f32.const ${v}`},0x44:()=>{const v=buf.readDoubleLE(p);p+=8;return `f64.const ${v}`},
  0x45:()=>'i32.eqz',0x46:()=>'i32.eq',0x47:()=>'i32.ne',0x48:()=>'i32.lt_s',0x49:()=>'i32.lt_u',0x4a:()=>'i32.gt_s',0x4b:()=>'i32.gt_u',0x4c:()=>'i32.le_s',0x4d:()=>'i32.le_u',0x4e:()=>'i32.ge_s',0x4f:()=>'i32.ge_u',
  0x50:()=>'i64.eqz',0x51:()=>'i64.eq',0x52:()=>'i64.ne',0x53:()=>'i64.lt_s',0x54:()=>'i64.lt_u',0x55:()=>'i64.gt_s',0x56:()=>'i64.gt_u',0x57:()=>'i64.le_s',0x58:()=>'i64.le_u',0x59:()=>'i64.ge_s',0x5a:()=>'i64.ge_u',
  0x5b:()=>'f32.eq',0x5c:()=>'f32.ne',0x5d:()=>'f32.lt',0x5e:()=>'f32.gt',0x5f:()=>'f32.le',0x60:()=>'f32.ge',
  0x61:()=>'f64.eq',0x62:()=>'f64.ne',0x63:()=>'f64.lt',0x64:()=>'f64.gt',0x65:()=>'f64.le',0x66:()=>'f64.ge',
  0x67:()=>'i32.clz',0x68:()=>'i32.ctz',0x69:()=>'i32.popcnt',
  0x6a:()=>'i32.add',0x6b:()=>'i32.sub',0x6c:()=>'i32.mul',0x6d:()=>'i32.div_s',0x6e:()=>'i32.div_u',0x6f:()=>'i32.rem_s',0x70:()=>'i32.rem_u',
  0x71:()=>'i32.and',0x72:()=>'i32.or',0x73:()=>'i32.xor',0x74:()=>'i32.shl',0x75:()=>'i32.shr_s',0x76:()=>'i32.shr_u',0x77:()=>'i32.rotl',0x78:()=>'i32.rotr',
  0x79:()=>'i64.clz',0x7a:()=>'i64.ctz',0x7b:()=>'i64.popcnt',
  0x7c:()=>'i64.add',0x7d:()=>'i64.sub',0x7e:()=>'i64.mul',0x7f:()=>'i64.div_s',0x80:()=>'i64.div_u',0x81:()=>'i64.rem_s',0x82:()=>'i64.rem_u',
  0x83:()=>'i64.and',0x84:()=>'i64.or',0x85:()=>'i64.xor',0x86:()=>'i64.shl',0x87:()=>'i64.shr_s',0x88:()=>'i64.shr_u',0x89:()=>'i64.rotl',0x8a:()=>'i64.rotr',
  0x8b:()=>'f32.abs',0x8c:()=>'f32.neg',0x91:()=>'f32.sqrt',0x92:()=>'f32.add',0x93:()=>'f32.sub',0x94:()=>'f32.mul',0x95:()=>'f32.div',
  0x99:()=>'f64.abs',0x9a:()=>'f64.neg',0x9f:()=>'f64.sqrt',0xa0:()=>'f64.add',0xa1:()=>'f64.sub',0xa2:()=>'f64.mul',0xa3:()=>'f64.div',
  0xa7:()=>'i32.wrap_i64',0xa8:()=>'i32.trunc_f32_s',0xa9:()=>'i32.trunc_f32_u',0xaa:()=>'i32.trunc_f64_s',0xab:()=>'i32.trunc_f64_u',
  0xac:()=>'i64.extend_i32_s',0xad:()=>'i64.extend_i32_u',0xae:()=>'i64.trunc_f32_s',0xb2:()=>'f32.convert_i32_s',0xb3:()=>'f32.convert_i32_u',0xb4:()=>'f32.convert_i64_s',0xb5:()=>'f32.convert_i64_u',
  0xb6:()=>'f32.demote_f64',0xb7:()=>'f64.convert_i32_s',0xb8:()=>'f64.convert_i32_u',0xb9:()=>'f64.convert_i64_s',0xba:()=>'f64.convert_i64_u',0xbb:()=>'f64.promote_f32',
  0xbc:()=>'i32.reinterpret_f32',0xbd:()=>'i64.reinterpret_f64',0xbe:()=>'f32.reinterpret_i32',0xbf:()=>'f64.reinterpret_i64',
  0xc0:()=>'i32.extend8_s',0xc1:()=>'i32.extend16_s',0xc2:()=>'i64.extend8_s',0xc3:()=>'i64.extend16_s',0xc4:()=>'i64.extend32_s',
  0xfc:()=>{const sub=Number(u());if(sub===10){p+=2;return 'memory.copy'}if(sub===11){p++;return 'memory.fill'}return `fc.${sub}`},
};
while (p < end) {
  const at = p, op = buf[p++];
  const f = OPS[op];
  if (!f) { console.log(`0x${at.toString(16)}: ?? 0x${op.toString(16)} — UNKNOWN, stopping`); break; }
  console.log(`0x${at.toString(16)}: ${f()}`);
}
