//! Module for reading and writing DEFLATE streams. Streams are read in as a vector of blocks containing tokens
//! can which can be written back out as an identical DEFLATE stream.

mod bit_reader;
mod bit_writer;

pub mod deflate_constants;

pub mod deflate_reader;
pub mod deflate_token;
pub mod deflate_writer;
pub mod huffman_calc;
pub mod huffman_encoding;
