use nerve_core::request_table::RequestTable;
use nerve_protocol::types::RequestId;

#[test]
fn cancel_unknown_request_is_safe() {
    let mut table = RequestTable::new();
    assert!(!table.cancel(RequestId(123)));
}

#[test]
fn duplicate_cancel_is_safe() {
    let mut table = RequestTable::new();
    let id = RequestId(1);

    table.insert(id);
    assert!(table.cancel(id));
    assert!(table.cancel(id));
}
