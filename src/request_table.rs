use std::collections::HashMap;

use nerve_protocol::{request, types::RequestId};

// state of request inside the core
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RequestState{
    Active,
    Cancelled,
}

/// track in-flight requests for a sigle connection
/// 
/// v0.1 responsibalities
/// -track active requests
/// mark cancelled requests
/// allow cleanups

pub struct RequestTable{
    requests: HashMap<RequestId, RequestState>,
}

impl RequestTable{
    // create a new empty request table
    #[inline]
    pub fn new() -> Self{
        Self{
            requests: HashMap::new(),
        }
    }

    // register a new request as active

    // returns false if request id already exists

    pub fn insert(&mut self, request_id: RequestId) -> bool{
        match self.requests.get(&request_id) {
            Some(_) => false,
            None => {
                self.requests.insert(request_id, RequestState::Active);
                true
            }
        }
    }

    // mark an existing request as cancelled
    // 
    // returns false if request_id does not exists

    pub fn cancel(&mut self, request_id: RequestId)-> bool{
        match self.requests.get_mut(&request_id) {
            Some(state) => {
                *state = RequestState::Cancelled;
                true
            }
            None => false,
        }
    }

    // check if the request has been cancelled
    pub fn is_cancelled(&self, request_id: RequestId)-> bool {
        matches!(
            self.requests.get(&request_id),
            Some(RequestState::Cancelled)
        )
    }

    // Remove the request from the table
    pub fn remove(&mut self, request_id: RequestId) {
        self.requests.remove(&request_id);
    }

    // number of tracked request (for diagnostics)
    pub fn len(&self) -> usize{
        self.requests.len()
    }

    // true if no in-flight requests
    pub fn is_empty(&self) -> bool {
        self.requests.is_empty()
    }
}
