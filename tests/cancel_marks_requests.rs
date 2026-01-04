use nerve_core::request_table::RequestTable;
use nerve_protocol::types::RequestId;

#[test]
fn cancel_marks_request(){
    let mut table = RequestTable::new();
    let id = RequestId(7);

    assert!(table.insert(id));
    assert!(!table.is_cancelled(id));

    assert!(table.cancel(id));
    assert!(table.is_cancelled(id));

    assert!(table.cancel(id))
}