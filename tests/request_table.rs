use nerve_core::request_table::RequestTable;
use nerve_protocol::types::RequestId;

#[test]
fn request_table_lifecycle() {
    let mut table = RequestTable::new();
    let id = RequestId(1);

    assert!(table.insert(id));
    assert!(!table.is_cancelled(id));

    assert!(table.cancel(id));
    assert!(table.is_cancelled(id));

    table.remove(id);
    assert!(table.is_empty());
}
