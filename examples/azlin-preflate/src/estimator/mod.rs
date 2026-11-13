//! This module is useed to estimate the parameters used to compress this DEFLATE stream. If we get these
//! parameters right, it will minimize or even eliminate the need to encode any corrections when we
//! recompress the stream.

mod complevel_estimator;
mod depth_estimator;
mod preflate_stream_info;

pub mod add_policy_estimator;
pub mod preflate_parameter_estimator;
pub mod preflate_parse_config;
