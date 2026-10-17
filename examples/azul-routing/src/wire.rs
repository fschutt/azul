//! The protobuf wire format, the little of it the routing tiles and an
//! OpenStreetMap extract use: varints, zig-zag signed numbers,
//! length-delimited fields and packed repeated numbers. Hand-written, so
//! reading a tile needs no code generator and no protobuf crate.

/// One field's value as it is on the wire.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Value<'a> {
    Varint(u64),
    Fixed64(u64),
    Bytes(&'a [u8]),
    Fixed32(u32),
}

/// Reads the fields of one message in turn.
pub struct Reader<'a> {
    /// The message's bytes.
    buf: &'a [u8],
    /// Where the next field starts.
    pos: usize,
}

impl<'a> Reader<'a> {
    /// Reads the message `buf` from its first field.
    #[must_use]
    pub const fn new(buf: &'a [u8]) -> Self {
        Self { buf, pos: 0 }
    }

    /// Whether every byte has been read.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.pos >= self.buf.len()
    }

    /// The next varint.
    ///
    /// # Errors
    /// The varint runs past the end or is longer than ten bytes.
    pub fn varint(&mut self) -> Result<u64, String> {
        let mut value = 0u64;
        for shift in (0..64).step_by(7) {
            let byte = *self.buf.get(self.pos).ok_or("a varint runs past the end")?;
            self.pos += 1;
            value |= u64::from(byte & 0x7f) << shift;
            if byte & 0x80 == 0 {
                return Ok(value);
            }
        }
        Err("a varint is longer than ten bytes".into())
    }

    fn take(&mut self, n: usize) -> Result<&'a [u8], String> {
        let end = self
            .pos
            .checked_add(n)
            .filter(|end| *end <= self.buf.len())
            .ok_or("a field runs past the end")?;
        let bytes = &self.buf[self.pos..end];
        self.pos = end;
        Ok(bytes)
    }

    /// The next field's number and value; `None` at the end of the message.
    ///
    /// # Errors
    /// The bytes are not a protobuf message.
    pub fn field(&mut self) -> Result<Option<(u32, Value<'a>)>, String> {
        if self.is_empty() {
            return Ok(None);
        }
        let key = self.varint()?;
        let number = u32::try_from(key >> 3).map_err(|_| "a field number is too big")?;
        let value = match key & 7 {
            0 => Value::Varint(self.varint()?),
            1 => Value::Fixed64(u64::from_le_bytes(self.take(8)?.try_into().map_err(|_| "fixed64")?)),
            2 => {
                let n = usize::try_from(self.varint()?).map_err(|_| "a length is too big")?;
                Value::Bytes(self.take(n)?)
            }
            5 => Value::Fixed32(u32::from_le_bytes(self.take(4)?.try_into().map_err(|_| "fixed32")?)),
            other => return Err(format!("wire type {other} is not used by the tiles")),
        };
        Ok(Some((number, value)))
    }
}

/// A zig-zag `sint64` back to its signed value.
#[must_use]
#[allow(clippy::cast_possible_wrap)] // zig-zag keeps the sign in the low bit
pub const fn unzigzag(n: u64) -> i64 {
    (n >> 1) as i64 ^ -((n & 1) as i64)
}

/// A signed value as zig-zag `sint64`.
#[must_use]
#[allow(clippy::cast_sign_loss)]
pub const fn zigzag(n: i64) -> u64 {
    ((n << 1) ^ (n >> 63)) as u64
}

/// Appends the numbers of a repeated field to `out`, packed (one
/// length-delimited run) or not (one varint per field), as protobuf allows
/// both.
///
/// # Errors
/// A packed run is cut off, or the field is neither a varint nor bytes.
pub fn read_varints(value: Value<'_>, out: &mut Vec<u64>) -> Result<(), String> {
    match value {
        Value::Varint(v) => out.push(v),
        Value::Bytes(bytes) => {
            let mut packed = Reader::new(bytes);
            while !packed.is_empty() {
                out.push(packed.varint()?);
            }
        }
        _ => return Err("a repeated number has the wrong wire type".into()),
    }
    Ok(())
}

/// Appends the values of a repeated `fixed64` field to `out`, packed (one
/// length-delimited run of 8-byte little-endian values) or not.
///
/// # Errors
/// A packed run is not a whole number of values, or the wire type is wrong.
pub fn read_fixed64s(value: Value<'_>, out: &mut Vec<u64>) -> Result<(), String> {
    match value {
        Value::Fixed64(v) => out.push(v),
        Value::Bytes(bytes) if bytes.len() % 8 == 0 => out.extend(
            bytes
                .chunks_exact(8)
                .map(|b| u64::from_le_bytes(b.try_into().unwrap_or_default())),
        ),
        _ => return Err("a repeated fixed64 has the wrong wire type or length".into()),
    }
    Ok(())
}

/// Writes one message.
#[derive(Debug, Default)]
pub struct Writer {
    /// The message so far.
    pub buf: Vec<u8>,
}

impl Writer {
    /// A bare varint, as a field's value or a packed run's element.
    pub fn varint(&mut self, mut v: u64) {
        while v >= 0x80 {
            #[allow(clippy::cast_possible_truncation)]
            self.buf.push((v as u8) | 0x80);
            v >>= 7;
        }
        #[allow(clippy::cast_possible_truncation)]
        self.buf.push(v as u8);
    }

    fn key(&mut self, number: u32, wire: u8) {
        self.varint((u64::from(number) << 3) | u64::from(wire));
    }

    /// A varint field; proto3 leaves out a zero.
    pub fn uint(&mut self, number: u32, v: u64) {
        if v != 0 {
            self.key(number, 0);
            self.varint(v);
        }
    }

    /// A `fixed64` field (a hash: as a varint it would take ten bytes);
    /// proto3 leaves out a zero.
    pub fn fixed64(&mut self, number: u32, v: u64) {
        if v != 0 {
            self.key(number, 1);
            self.buf.extend_from_slice(&v.to_le_bytes());
        }
    }

    /// A packed repeated `fixed64` field; an empty one is left out.
    pub fn packed_fixed64(&mut self, number: u32, values: &[u64]) {
        if !values.is_empty() {
            let run: Vec<u8> = values.iter().flat_map(|v| v.to_le_bytes()).collect();
            self.bytes(number, &run);
        }
    }

    /// A length-delimited field: a string, bytes or a nested message.
    pub fn bytes(&mut self, number: u32, bytes: &[u8]) {
        self.key(number, 2);
        self.varint(bytes.len() as u64);
        self.buf.extend_from_slice(bytes);
    }

    /// A packed repeated varint field; an empty one is left out.
    pub fn packed(&mut self, number: u32, values: impl IntoIterator<Item = u64>) {
        let mut run = Self::default();
        for v in values {
            run.varint(v);
        }
        if !run.buf.is_empty() {
            self.bytes(number, &run.buf);
        }
    }

    /// A packed repeated `sint64` field holding each value's difference to
    /// the one before (the first to zero), like the OpenStreetMap PBF's
    /// dense nodes: sorted ids and nearby coordinates become small numbers.
    pub fn delta(&mut self, number: u32, values: impl IntoIterator<Item = i64>) {
        let mut last = 0i64;
        self.packed(
            number,
            values.into_iter().map(|v| {
                let d = v.wrapping_sub(last);
                last = v;
                zigzag(d)
            }),
        );
    }
}

/// The values of a delta-coded `sint64` column, summed back up.
#[must_use]
pub fn undelta(raw: &[u64]) -> Vec<i64> {
    let mut last = 0i64;
    raw.iter()
        .map(|&v| {
            last = last.wrapping_add(unzigzag(v));
            last
        })
        .collect()
}
