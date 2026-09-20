use tempfile::tempdir;

use workbuddy_auto_signin::instance_lock::{InstanceLock, InstanceLockError};

#[test]
fn second_instance_is_rejected_until_first_lock_is_dropped() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("run.lock");

    let first = InstanceLock::acquire_at(&path).unwrap();
    assert_eq!(first.path(), path.as_path());
    assert!(matches!(
        InstanceLock::acquire_at(&path),
        Err(InstanceLockError::Busy)
    ));

    drop(first);
    InstanceLock::acquire_at(&path).unwrap();
}
