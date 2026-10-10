#!/usr/bin/env python3
"""Icon-O-Matic documents -> HVIF, byte for byte as Icon-O-Matic exports them.

The Haiku project keeps its icons in `data/artwork/icons` as Icon-O-Matic's
NATIVE documents: the magic `IMSG` and a flattened BMessage archive of the
icon's paths, styles and shapes (the editor's own file). What Haiku installs
- and what azul draws - is HVIF (`ncif`), the compact format Icon-O-Matic
writes with "Export > HVIF". This script is that export, ported from Haiku's
sources so the conversion adds nothing and leaves nothing out:

- reading: src/libs/icon/message/MessageImporter.cpp, the archive
  constructors of VectorPath, Style, Gradient (GradientTransformable.cpp),
  PathSourceShape / Shape::Unarchive and the four transformers, over the
  BMessage formats Haiku reads (src/kits/app/Message.cpp: HMF1;
  src/kits/app/MessageAdapter.cpp: R5);
- writing: src/apps/icon-o-matic/import_export/flat_icon/FlatIconExporter.cpp
  with src/libs/icon/flat_icon/FlatIconFormat.cpp (write_coord,
  write_float_24) and PathCommandQueue.cpp - including their rounding.

Usage: iom2hvif.py INPUT OUTPUT.hvif
"""

import struct
import sys

# --- BMessage --------------------------------------------------------------

B_MESSAGE_TYPE = 0x4D534747  # 'MSGG'


class Message:
    """A flattened BMessage: its `what` and its fields, name -> (type, [item bytes])."""

    def __init__(self, what):
        self.what = what
        self.fields = {}

    def add(self, name, type_code, item):
        entry = self.fields.setdefault(name, (type_code, []))
        entry[1].append(item)

    def items(self, name):
        entry = self.fields.get(name)
        return entry[1] if entry else []

    def find(self, name, index=0):
        items = self.items(name)
        return items[index] if index < len(items) else None

    def find_int32(self, name, index=0):
        item = self.find(name, index)
        return None if item is None else struct.unpack("<i", item[:4])[0]

    def find_float(self, name, index=0):
        item = self.find(name, index)
        return None if item is None else struct.unpack("<f", item[:4])[0]

    def find_double(self, name, index=0):
        item = self.find(name, index)
        return None if item is None else struct.unpack("<d", item[:8])[0]

    def find_bool(self, name, index=0):
        item = self.find(name, index)
        return None if item is None else item[0] != 0

    def find_point(self, name, index=0):
        item = self.find(name, index)
        return None if item is None else struct.unpack("<ff", item[:8])

    def find_message(self, name, index=0):
        item = self.find(name, index)
        return None if item is None else unflatten(item)

    def find_doubles(self, name, count):
        """`FindData(name, B_DOUBLE_TYPE)` of one item holding `count` doubles."""
        item = self.find(name)
        if item is None or len(item) != count * 8:
            return None
        return list(struct.unpack("<%dd" % count, item))


def unflatten(data):
    """BMessage::Unflatten: the format is the first uint32 (little-endian here)."""
    magic = data[:4]
    if magic == b"HMF1":  # MESSAGE_FORMAT_HAIKU ('1FMH' little-endian)
        return _unflatten_haiku(data)
    if magic == b"1BOF":  # MESSAGE_FORMAT_R5 ('FOB1' little-endian)
        return _unflatten_r5(data)
    raise ValueError("unsupported BMessage format %r" % magic)


def _unflatten_haiku(data):
    # message_header: format, what, flags, target, current_specifier,
    # message_area, reply_port, reply_target, reply_team, data_size,
    # field_count, hash_table_size, hash_table[5] (MessagePrivate.h).
    header = struct.unpack_from("<12I5i", data, 0)
    what, data_size, field_count = header[1], header[9], header[10]
    fields_at = 12 * 4 + 5 * 4
    data_at = fields_at + field_count * 24
    message = Message(what)
    for i in range(field_count):
        flags, name_length, type_code, count, size, offset, _next = struct.unpack_from(
            "<HHIIIIi", data, fields_at + i * 24
        )
        at = data_at + offset
        name = data[at:at + name_length].split(b"\0", 1)[0].decode("utf-8")
        at += name_length
        if flags & 0x0002:  # FIELD_FLAG_FIXED_SIZE
            item_size = size // count
            for k in range(count):
                message.add(name, type_code, data[at + k * item_size:at + (k + 1) * item_size])
        else:
            for _ in range(count):
                (item_size,) = struct.unpack_from("<I", data, at)
                at += 4
                message.add(name, type_code, data[at:at + item_size])
                at += item_size
    assert data_at + data_size <= len(data), "truncated message"
    return message


def _pad_to_8(value):
    return (value + 7) & ~7


def _unflatten_r5(data):
    # r5_message_header: magic, checksum, flattened_size, what, flags (u8).
    _magic, _checksum, _size, what, flags = struct.unpack_from("<IIiiB", data, 0)
    at = 17
    if flags & 0x02:  # R5_MESSAGE_FLAG_INCLUDE_TARGET
        at += 4
    if flags & 0x04:  # R5_MESSAGE_FLAG_INCLUDE_REPLY: port, target, team, 4 flag bytes
        at += 12 + 4
    message = Message(what)
    while True:
        field_flags = data[at]
        at += 1
        if not field_flags & 0x01:  # R5_FIELD_FLAG_VALID
            break
        fixed = field_flags & 0x04
        mini = field_flags & 0x02
        single = field_flags & 0x08
        (type_code,) = struct.unpack_from("<I", data, at)
        at += 4
        if single:
            count = 1
        elif mini:
            count = data[at]
            at += 1
        else:
            (count,) = struct.unpack_from("<i", data, at)
            at += 4
        if mini:
            size = data[at]
            at += 1
        else:
            (size,) = struct.unpack_from("<i", data, at)
            at += 4
        name_length = data[at]
        at += 1
        name = data[at:at + name_length].decode("utf-8")
        at += name_length
        body = data[at:at + size]
        at += size
        pointer = 0
        for _ in range(count):
            if fixed:
                item_size = size // count
                message.add(name, type_code, body[pointer:pointer + item_size])
                pointer += item_size
            else:
                (item_size,) = struct.unpack_from("<i", body, pointer)
                pointer += 4
                message.add(name, type_code, body[pointer:pointer + item_size])
                pointer += _pad_to_8(item_size + 4) - 4
    return message


# --- the icon (libicon's archive constructors) --------------------------------

IDENTITY = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0]  # sx, shy, shx, sy, tx, ty (agg store_to)

ARCHIVE_PATH_SOURCE_SHAPE = 0x73687073  # 'shps'
ARCHIVE_AFFINE = 0x6166666E  # 'affn'
ARCHIVE_CONTOUR = 0x636E7472  # 'cntr'
ARCHIVE_PERSPECTIVE = 0x70727370  # 'prsp'
ARCHIVE_STROKE = 0x7374726B  # 'strk'


def _rgba(item):
    """An rgb_color archived with AddInt32: its four bytes, red first."""
    return tuple(item[:4])


def read_path(archive):
    """VectorPath(BMessage*): as many points as `point` has; a point whose
    fields run out early stays zeroed, as the zeroed allocation leaves it."""
    found = archive.fields.get("point")
    count = len(found[1]) if found else 0
    origin = (0.0, 0.0)
    points = [(origin, origin, origin)] * count
    for i in range(count):
        point = archive.find_point("point", i)
        point_in = archive.find_point("point in", i)
        point_out = archive.find_point("point out", i)
        connected = archive.find_bool("connected", i)
        if None in (point, point_in, point_out, connected):
            break
        points[i] = (point, point_in, point_out)
    closed = archive.find_bool("path closed")
    return {"points": points, "closed": bool(closed)}


def read_gradient(archive):
    matrix = archive.find_doubles("transformation", 6) or list(IDENTITY)
    stops = []
    i = 0
    while True:
        offset = archive.find_float("offset", i)
        if offset is None:
            break
        color = archive.find("color", i)
        if color is None:
            break
        # Gradient::AddColor(color, offset): sorted by offset, a stop
        # going after every stop of an equal offset.
        index = 0
        while index < len(stops) and not stops[index][0] > offset:
            index += 1
        stops.insert(index, (offset, _rgba(color)))
        i += 1
    kind = archive.find_int32("type")
    return {"type": 0 if kind is None else kind, "matrix": matrix, "stops": stops}


def read_style(archive):
    color = archive.find("color")
    style = {"color": _rgba(color) if color is not None else (255, 255, 255, 255)}
    gradient = archive.find_message("gradient")
    if gradient is not None:
        style["gradient"] = read_gradient(gradient)
    return style


def read_transformer(archive):
    what = archive.what
    if what == ARCHIVE_AFFINE:
        return {"kind": "affine", "matrix": archive.find_doubles("matrix", 6) or list(IDENTITY)}
    if what == ARCHIVE_PERSPECTIVE:
        matrix = [archive.find_double("matrix", i) for i in range(9)]
        return {"kind": "perspective", "matrix": [0.0 if v is None else v for v in matrix]}
    if what in (ARCHIVE_CONTOUR, ARCHIVE_STROKE):
        # agg's defaults where the archive says nothing (math_stroke: width 1,
        # miter join, butt cap, miter limit 4).
        join = archive.find_int32("line join")
        width = archive.find_double("width")
        miter = archive.find_double("miter limit")
        transformer = {
            "kind": "contour" if what == ARCHIVE_CONTOUR else "stroke",
            "width": 1.0 if width is None else width,
            "join": 0 if join is None else join,
            "miter_limit": 4.0 if miter is None else miter,
        }
        if what == ARCHIVE_STROKE:
            cap = archive.find_int32("line cap")
            transformer["cap"] = 0 if cap is None else cap
        return transformer
    return None  # TransformerFactory knows no other kind: dropped


def read_shape(archive, styles, paths):
    style_index = archive.find_int32("style ref")
    if style_index is None or not 0 <= style_index < len(styles):
        return None
    shape_paths = []
    j = 0
    while True:
        path_index = archive.find_int32("path ref", j)
        if path_index is None:
            break
        if 0 <= path_index < len(paths):
            shape_paths.append(path_index)
        j += 1
    transformers = []
    j = 0
    while True:
        message = archive.find_message("transformer", j)
        if message is None:
            break
        transformer = read_transformer(message)
        if transformer is not None:
            transformers.append(transformer)
        j += 1
    matrix = archive.find_doubles("transformation", 6) or list(IDENTITY)
    lo = archive.find_float("min visibility scale")
    hi = archive.find_float("max visibility scale")
    lo = 0.0 if lo is None else min(max(lo, 0.0), 4.0)
    hi = 4.0 if hi is None else min(max(hi, 0.0), 4.0)
    return {
        "style": style_index,
        "paths": shape_paths,
        "matrix": matrix,
        "hinting": bool(archive.find_bool("hinting")),
        "min_scale": lo,
        "max_scale": hi,
        "transformers": transformers,
    }


def read_native(data):
    """MessageImporter::Import: an Icon-O-Matic document -> paths, styles, shapes."""
    if data[:4] == b"IMSG":  # kNativeIconMagicNumber, big-endian
        data = data[4:]
    archive = unflatten(data)
    paths = [read_path(m) for m in _messages(archive.find_message("paths"), "path")]
    styles = [read_style(m) for m in _messages(archive.find_message("styles"), "style")]
    shapes = []
    for shape_archive in _messages(archive.find_message("shapes"), "shape"):
        kind = shape_archive.find_int32("type")
        if kind is not None and kind != ARCHIVE_PATH_SOURCE_SHAPE:
            continue  # a reference image: the editor's, never exported
        shape = read_shape(shape_archive, styles, paths)
        if shape is not None:
            shapes.append(shape)
    return {"paths": paths, "styles": styles, "shapes": shapes}


def _messages(container, name):
    if container is None:
        raise ValueError("no `%s` list in the document" % name)
    return [unflatten(item) for item in container.items(name)]


# --- HVIF (FlatIconExporter) ---------------------------------------------------


def _f32(value):
    """A double stored into a C `float`."""
    return struct.unpack("<f", struct.pack("<f", value))[0]


def write_coord(out, coord):
    coord = _f32(coord)
    if coord < -128.0:
        coord = -128.0
    if coord > 192.0:
        coord = 192.0
    if int(coord * 100.0) == int(coord) * 100 and -32.0 <= coord <= 95.0:
        out.append(int(coord + 32.0) & 0xFF)
    else:
        value = int((coord + 128.0) * 102.0) & 0xFFFF
        value |= 0x8000
        out.append(value >> 8)
        out.append(value & 0xFF)


def write_float_24(out, value):
    bits = struct.unpack("<I", struct.pack("<f", value))[0]
    sign = (bits & 0x80000000) >> 31
    exponent = ((bits & 0x7F800000) >> 23) - 127
    mantissa = bits & 0x007FFFFF
    if exponent >= 32 or exponent < -32:
        out += b"\0\0\0"
        return
    short = (sign << 23) | ((exponent + 32) << 17) | (mantissa >> 6)
    out.append((short >> 16) & 0xFF)
    out.append((short >> 8) & 0xFF)
    out.append(short & 0xFF)


def _write_matrix(out, matrix):
    for value in matrix:
        write_float_24(out, value)


def _write_styles(out, styles):
    if len(styles) > 255:
        raise ValueError("too many styles")
    out.append(len(styles))
    for style in styles:
        gradient = style.get("gradient")
        r, g, b, a = style["color"]
        if gradient is not None:
            out.append(2)  # STYLE_TYPE_GRADIENT
            _write_gradient(out, gradient)
        elif r == g == b:
            if a == 255:
                out += bytes([5, r])  # STYLE_TYPE_SOLID_GRAY_NO_ALPHA
            else:
                out += bytes([4, r, a])  # STYLE_TYPE_SOLID_GRAY
        elif a == 255:
            out += bytes([3, r, g, b])  # STYLE_TYPE_SOLID_COLOR_NO_ALPHA
        else:
            out += bytes([1, r, g, b, a])  # STYLE_TYPE_SOLID_COLOR


def _write_gradient(out, gradient):
    stops = gradient["stops"]
    flags = 0
    if gradient["matrix"] != IDENTITY:
        flags |= 1 << 1  # GRADIENT_FLAG_TRANSFORM
    alpha = any(c[3] < 255 for _, c in stops)
    gray = all(c[0] == c[1] == c[2] for _, c in stops)
    if not alpha:
        flags |= 1 << 2  # GRADIENT_FLAG_NO_ALPHA
    if gray:
        flags |= 1 << 4  # GRADIENT_FLAG_GRAYS
    out += bytes([gradient["type"] & 0xFF, flags, len(stops) & 0xFF])
    if flags & (1 << 1):
        _write_matrix(out, gradient["matrix"])
    for offset, (r, g, b, a) in stops:
        out.append(int(_f32(offset) * 255.0) & 0xFF)
        if alpha:
            out += bytes([r, a]) if gray else bytes([r, g, b, a])
        else:
            out += bytes([r]) if gray else bytes([r, g, b])


def _corner(point, point_in, point_out):
    return point == point_in and point == point_out


def _write_paths(out, paths):
    if len(paths) > 255:
        raise ValueError("too many paths")
    out.append(len(paths))
    for path in paths:
        points = path["points"]
        if len(points) > 255:
            raise ValueError("a path with too many points")
        flags = 1 << 1 if path["closed"] else 0  # PATH_FLAG_CLOSED
        straight = line = curve = 0
        last = (0.0, 0.0)
        for point, point_in, point_out in points:
            if _corner(point, point_in, point_out):
                if point[0] == last[0] or point[1] == last[1]:
                    straight += 1
                else:
                    line += 1
            else:
                curve += 1
            last = point
        count = len(points)
        if count + straight * 2 + line * 4 + curve * 12 < count * 12:
            flags |= (1 << 3) if curve == 0 else (1 << 2)  # NO_CURVES / USES_COMMANDS
        out += bytes([flags, count])
        if flags & (1 << 3):
            for point, _, _ in points:
                write_coord(out, point[0])
                write_coord(out, point[1])
        elif flags & (1 << 2):
            _write_commands(out, points)
        else:
            for point, point_in, point_out in points:
                for x, y in (point, point_in, point_out):
                    write_coord(out, x)
                    write_coord(out, y)


def _write_commands(out, points):
    """PathCommandQueue::Write: two bits a command, then the coordinates."""
    commands = bytearray()
    coords = bytearray()
    byte = 0
    position = 0
    last = (0.0, 0.0)
    for point, point_in, point_out in points:
        if _corner(point, point_in, point_out):
            if point[0] == last[0]:
                command = 1  # V_LINE
                write_coord(coords, point[1])
            elif point[1] == last[1]:
                command = 0  # H_LINE
                write_coord(coords, point[0])
            else:
                command = 2  # LINE
                write_coord(coords, point[0])
                write_coord(coords, point[1])
        else:
            command = 3  # CURVE
            for x, y in (point, point_in, point_out):
                write_coord(coords, x)
                write_coord(coords, y)
        byte |= command << position
        position += 2
        if position == 8:
            commands.append(byte)
            byte = 0
            position = 0
        last = point
    if position > 0:
        commands.append(byte)
    out += commands
    out += coords


def _int8(value):
    """C's `(int8)` of a double: toward zero."""
    return int(value)


def _write_transformer(out, transformer):
    kind = transformer["kind"]
    if kind == "affine":
        out.append(20)
        _write_matrix(out, transformer["matrix"])
    elif kind == "contour":
        out.append(21)
        out.append((_int8(transformer["width"]) + 128) & 0xFF)
        out.append(transformer["join"] & 0xFF)
        out.append(int(transformer["miter_limit"]) & 0xFF)
    elif kind == "perspective":
        out.append(22)
        _write_matrix(out, transformer["matrix"])
    elif kind == "stroke":
        out.append(23)
        out.append((_int8(transformer["width"]) + 128) & 0xFF)
        out.append((transformer["join"] & 0xFF) | ((transformer["cap"] & 0xFF) << 4) & 0xFF)
        out.append(int(transformer["miter_limit"]) & 0xFF)


def _write_shapes(out, shapes):
    if len(shapes) > 255:
        raise ValueError("too many shapes")
    out.append(len(shapes))
    for shape in shapes:
        out += bytes([10, shape["style"], len(shape["paths"])])  # SHAPE_TYPE_PATH_SOURCE
        out += bytes(shape["paths"])
        matrix = shape["matrix"]
        flags = 0
        if matrix != IDENTITY:
            if matrix[:4] == IDENTITY[:4]:
                flags |= 1 << 5  # SHAPE_FLAG_TRANSLATION
            else:
                flags |= 1 << 1  # SHAPE_FLAG_TRANSFORM
        if shape["hinting"]:
            flags |= 1 << 2  # SHAPE_FLAG_HINTING
        if shape["min_scale"] != 0.0 or shape["max_scale"] != 4.0:
            flags |= 1 << 3  # SHAPE_FLAG_LOD_SCALE
        if shape["transformers"]:
            flags |= 1 << 4  # SHAPE_FLAG_HAS_TRANSFORMERS
        out.append(flags)
        if flags & (1 << 1):
            _write_matrix(out, matrix)
        elif flags & (1 << 5):
            # Transform(&B_ORIGIN) into a BPoint: the translation as floats.
            write_coord(out, _f32(matrix[4]))
            write_coord(out, _f32(matrix[5]))
        if flags & (1 << 3):
            out.append(int(shape["min_scale"] * 63.75 + 0.5) & 0xFF)
            out.append(int(shape["max_scale"] * 63.75 + 0.5) & 0xFF)
        if flags & (1 << 4):
            out.append(len(shape["transformers"]))
            for transformer in shape["transformers"]:
                _write_transformer(out, transformer)


def export_hvif(icon):
    """FlatIconExporter::_Export: `ncif`, the styles, the paths, the shapes."""
    out = bytearray(b"ncif")
    _write_styles(out, icon["styles"])
    _write_paths(out, icon["paths"])
    _write_shapes(out, icon["shapes"])
    return bytes(out)


def convert(data):
    return export_hvif(read_native(data))


if __name__ == "__main__":
    if len(sys.argv) != 3:
        sys.exit(__doc__)
    with open(sys.argv[1], "rb") as source:
        hvif = convert(source.read())
    with open(sys.argv[2], "wb") as target:
        target.write(hvif)
