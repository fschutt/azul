//! A bucket that misbehaves where a test asks it to, over any other bucket: a write that is
//! refused, a write that lands and whose answer is lost (or is answered "the object is there",
//! as the resend of a conditional write is), a delete that fails, an object that is bigger than
//! its size. For the tests of the delta review of the key lanes (X-DELTA-AZDRIVE-KEYS-*).

use std::sync::{Arc, Mutex, PoisonError};

use crate::{
    limits::Limit, ByteRange, Drive, DriveError, ListPage, ListRequest, ObjectInfo, Precondition,
};

/// How a write goes wrong.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Fail {
    /// The bucket does not take it: nothing is stored.
    Refused,
    /// It is stored, and the answer never comes back (`Transport`).
    AnswerLost,
    /// It is stored, and the answer is "the object is there already" (`Conflict`): the answer a
    /// resend of a conditional write gets after the first one landed.
    AnswerConflict,
}

#[derive(Debug)]
struct Fault {
    key: String,
    how: Fail,
    times: u32,
}

/// `inner`, with faults.
pub(crate) struct Faulty {
    inner: Arc<dyn Drive>,
    /// Faults of a write (plain or conditional) of a key, in the order they were asked for.
    writes: Mutex<Vec<Fault>>,
    /// Keys whose delete fails (`Transport`), with how many times.
    deletes: Mutex<Vec<(String, u32)>>,
    /// Keys whose GET (and ranged GET) fails (`Transport`), with how many times.
    reads: Mutex<Vec<(String, u32)>>,
}

impl Faulty {
    pub(crate) fn new(inner: Arc<dyn Drive>) -> Faulty {
        Faulty {
            inner,
            writes: Mutex::new(Vec::new()),
            deletes: Mutex::new(Vec::new()),
            reads: Mutex::new(Vec::new()),
        }
    }

    /// The next `times` writes of `key` go wrong as `how`.
    pub(crate) fn fail_write(&self, key: &str, how: Fail, times: u32) {
        self.writes
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(Fault {
                key: key.to_string(),
                how,
                times,
            });
    }

    /// The next `times` deletes of `key` fail.
    pub(crate) fn fail_delete(&self, key: &str, times: u32) {
        self.deletes
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push((key.to_string(), times));
    }

    /// The next `times` reads of `key` fail.
    pub(crate) fn fail_read(&self, key: &str, times: u32) {
        self.reads
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push((key.to_string(), times));
    }

    fn write_fault(&self, key: &str) -> Option<Fail> {
        let mut faults = self.writes.lock().unwrap_or_else(PoisonError::into_inner);
        let fault = faults
            .iter_mut()
            .find(|fault| fault.key == key && fault.times > 0)?;
        fault.times -= 1;
        Some(fault.how)
    }

    fn counted(list: &Mutex<Vec<(String, u32)>>, key: &str) -> bool {
        let mut list = list.lock().unwrap_or_else(PoisonError::into_inner);
        match list.iter_mut().find(|(k, left)| k == key && *left > 0) {
            Some((_, left)) => {
                *left -= 1;
                true
            }
            None => false,
        }
    }

    fn went() -> DriveError {
        DriveError::Transport(String::from("the network went"))
    }
}

impl Drive for Faulty {
    fn list(&self, request: &ListRequest) -> Result<ListPage, DriveError> {
        self.inner.list(request)
    }

    fn get(&self, key: &str) -> Result<Vec<u8>, DriveError> {
        if Self::counted(&self.reads, key) {
            return Err(Self::went());
        }
        self.inner.get(key)
    }

    fn get_range(&self, key: &str, range: ByteRange) -> Result<Vec<u8>, DriveError> {
        if Self::counted(&self.reads, key) {
            return Err(Self::went());
        }
        self.inner.get_range(key, range)
    }

    fn get_limited(&self, key: &str, limit: Limit) -> Result<Vec<u8>, DriveError> {
        if Self::counted(&self.reads, key) {
            return Err(Self::went());
        }
        self.inner.get_limited(key, limit)
    }

    fn put(&self, key: &str, bytes: &[u8]) -> Result<(), DriveError> {
        match self.write_fault(key) {
            None => self.inner.put(key, bytes),
            Some(Fail::Refused) => Err(Self::went()),
            Some(Fail::AnswerLost) => {
                self.inner.put(key, bytes)?;
                Err(Self::went())
            }
            Some(Fail::AnswerConflict) => {
                self.inner.put(key, bytes)?;
                Err(DriveError::Conflict {
                    key: key.to_string(),
                })
            }
        }
    }

    fn delete(&self, key: &str) -> Result<(), DriveError> {
        if Self::counted(&self.deletes, key) {
            return Err(Self::went());
        }
        self.inner.delete(key)
    }

    fn head(&self, key: &str) -> Result<ObjectInfo, DriveError> {
        self.inner.head(key)
    }

    fn put_if(
        &self,
        key: &str,
        bytes: &[u8],
        condition: &Precondition,
    ) -> Result<Option<String>, DriveError> {
        match self.write_fault(key) {
            None => self.inner.put_if(key, bytes, condition),
            Some(Fail::Refused) => Err(Self::went()),
            Some(Fail::AnswerLost) => {
                self.inner.put_if(key, bytes, condition)?;
                Err(Self::went())
            }
            Some(Fail::AnswerConflict) => {
                self.inner.put_if(key, bytes, condition)?;
                Err(DriveError::Conflict {
                    key: key.to_string(),
                })
            }
        }
    }

    fn conditional_writes(&self) -> bool {
        self.inner.conditional_writes()
    }
}
