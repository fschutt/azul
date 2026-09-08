//! Unified `Db` handle. See [`crate::unified`].
//!
//! Off-wasm this is the turso-backed store in `desktop::extra::sqlite`. On
//! wasm the browser host serves the same surface from IndexedDB; the stub
//! below only exists so the C-ABI exports have an identical `#[repr(C)]`
//! layout to transmute through, and every request it receives resolves
//! honestly (`DbErrorKind::NoEngine` / `Disconnected`) instead of hanging.

#[cfg(all(feature = "cabi_internal", not(target_arch = "wasm32")))]
pub use crate::desktop::extra::sqlite::*;

#[cfg(target_arch = "wasm32")]
use core::ffi::c_void;

#[cfg(target_arch = "wasm32")]
use azul_core::{
    db::{
        DbConfig, DbError, DbErrorKind, DbKeyRange, DbMergeCallback, DbRows, DbRowsResult, DbScope,
        DbSyncStatus, DbSyncStatusResult, DbValue, DbValueResult, DbValueVec, OptionDbScope,
        OptionDbValue,
    },
    refany::RefAny,
    task::RequestId,
};
#[cfg(target_arch = "wasm32")]
use azul_css::{
    corety::OptionString, impl_option, impl_option_inner, impl_result_inner, AzString, StringVec,
};
#[cfg(target_arch = "wasm32")]
use azul_layout::{callbacks::ResumeCallback, request};

/// wasm stub of the desktop `Db` handle; `#[repr(C)]` layout MUST match.
#[cfg(target_arch = "wasm32")]
#[repr(C)]
#[derive(Debug)]
pub struct Db {
    pub ptr: *mut c_void,
    pub run_destructor: bool,
}

#[cfg(target_arch = "wasm32")]
impl Clone for Db {
    fn clone(&self) -> Self {
        Db {
            ptr: self.ptr,
            run_destructor: false,
        }
    }
}

#[cfg(target_arch = "wasm32")]
impl Default for Db {
    fn default() -> Self {
        Db {
            ptr: core::ptr::null_mut(),
            run_destructor: false,
        }
    }
}

#[cfg(target_arch = "wasm32")]
impl Drop for Db {
    fn drop(&mut self) {}
}

#[cfg(target_arch = "wasm32")]
azul_css::impl_result!(
    Db,
    DbError,
    ResultDbDbError,
    copy = false,
    clone = false,
    [Debug, Clone]
);

/// wasm stub of `DbOpenResult`; layout MUST match the desktop type.
#[cfg(target_arch = "wasm32")]
#[repr(C)]
#[derive(Debug, Clone)]
pub struct DbOpenResult {
    pub result: ResultDbDbError,
}

#[cfg(target_arch = "wasm32")]
impl_option!(DbOpenResult, OptionDbOpenResult, copy = false, [Debug, Clone]);

#[cfg(target_arch = "wasm32")]
impl DbOpenResult {
    pub fn downcast(mut result: RefAny) -> OptionDbOpenResult {
        result.downcast_ref::<Self>().map(|r| r.clone()).into()
    }
}

#[cfg(target_arch = "wasm32")]
fn no_engine() -> AzString {
    AzString::from_const_str("no local db engine in this build (the web host serves Db)")
}

#[cfg(target_arch = "wasm32")]
fn empty_rows() -> DbRows {
    DbRows {
        columns: StringVec::from_vec(Vec::new()),
        values: DbValueVec::from_vec(Vec::new()),
    }
}

#[cfg(target_arch = "wasm32")]
impl Db {
    pub fn open(_config: DbConfig, data: RefAny, on_open: ResumeCallback) -> RequestId {
        request::complete(
            data,
            on_open,
            DbOpenResult {
                result: ResultDbDbError::Err(DbError::new(DbErrorKind::NoEngine, no_engine())),
            },
        )
    }
    pub fn is_open(&self) -> bool {
        false
    }
    pub fn get(&self, _store: AzString, _key: DbValue, data: RefAny, on_result: ResumeCallback) -> RequestId {
        request::complete(
            data,
            on_result,
            DbValueResult {
                value: OptionDbValue::None,
                error: OptionString::Some(no_engine()),
            },
        )
    }
    pub fn set(&self, _store: AzString, _key: DbValue, _value: DbValue) -> bool {
        false
    }
    pub fn remove(&self, _store: AzString, _key: DbValue) -> bool {
        false
    }
    pub fn iterate(
        &self,
        _store: AzString,
        _range: DbKeyRange,
        _limit: u32,
        data: RefAny,
        on_result: ResumeCallback,
    ) -> RequestId {
        request::complete(
            data,
            on_result,
            DbRowsResult {
                rows: empty_rows(),
                error: OptionString::Some(no_engine()),
            },
        )
    }
    pub fn query_index(
        &self,
        _store: AzString,
        _index: AzString,
        _range: DbKeyRange,
        _limit: u32,
        data: RefAny,
        on_result: ResumeCallback,
    ) -> RequestId {
        request::complete(
            data,
            on_result,
            DbRowsResult {
                rows: empty_rows(),
                error: OptionString::Some(no_engine()),
            },
        )
    }
    pub fn subscribe(&self, _scope: DbScope, _data: RefAny, _on_change: ResumeCallback) -> RequestId {
        RequestId::unique()
    }
    pub fn sync_now(&self, _scope: OptionDbScope, data: RefAny, on_result: ResumeCallback) -> RequestId {
        request::complete(
            data,
            on_result,
            DbSyncStatusResult {
                status: DbSyncStatus::disconnected(),
            },
        )
    }
    pub fn sync_status(&self) -> DbSyncStatus {
        DbSyncStatus::disconnected()
    }
    pub fn set_on_sync_status(&mut self, _data: RefAny, _on_status: ResumeCallback) {}
    pub fn set_on_conflict(&mut self, _store: AzString, _data: RefAny, _on_merge: DbMergeCallback) {}
    pub fn close(&mut self) {}
}
