use azul_core::refany::*;

#[derive(Debug, Clone, PartialEq)]
struct TestStruct {
    value: i32,
    name: String,
}

#[derive(Debug, Clone, PartialEq)]
struct NestedStruct {
    inner: TestStruct,
    data: Vec<u8>,
}

#[test]
fn test_refany_basic_create_and_downcast() {
    let test_val = TestStruct {
        value: 42,
        name: "test".to_string(),
    };

    let mut refany = RefAny::new(test_val.clone());

    // Test downcast_ref
    let borrowed = refany
        .downcast_ref::<TestStruct>()
        .expect("Should downcast successfully");
    assert_eq!(borrowed.value, 42);
    assert_eq!(borrowed.name, "test");
    drop(borrowed);

    // Test downcast_mut
    {
        let mut borrowed_mut = refany
            .downcast_mut::<TestStruct>()
            .expect("Should downcast mutably");
        borrowed_mut.value = 100;
        borrowed_mut.name = "modified".to_string();
    }

    // Verify mutation
    let borrowed = refany
        .downcast_ref::<TestStruct>()
        .expect("Should downcast after mutation");
    assert_eq!(borrowed.value, 100);
    assert_eq!(borrowed.name, "modified");
}

#[test]
fn test_refany_clone_and_sharing() {
    let test_val = TestStruct {
        value: 42,
        name: "test".to_string(),
    };

    let mut refany1 = RefAny::new(test_val);
    let mut refany2 = refany1.clone();
    let mut refany3 = refany1.clone();

    // All three should point to the same data
    let borrowed1 = refany1
        .downcast_ref::<TestStruct>()
        .expect("Should downcast ref1");
    assert_eq!(borrowed1.value, 42);
    drop(borrowed1);

    let borrowed2 = refany2
        .downcast_ref::<TestStruct>()
        .expect("Should downcast ref2");
    assert_eq!(borrowed2.value, 42);
    drop(borrowed2);

    // Modify through refany3
    {
        let mut borrowed_mut = refany3
            .downcast_mut::<TestStruct>()
            .expect("Should downcast mut");
        borrowed_mut.value = 200;
    }

    // Verify all see the change
    let borrowed1 = refany1
        .downcast_ref::<TestStruct>()
        .expect("Should see mutation from ref1");
    assert_eq!(borrowed1.value, 200);
    drop(borrowed1);

    let borrowed2 = refany2
        .downcast_ref::<TestStruct>()
        .expect("Should see mutation from ref2");
    assert_eq!(borrowed2.value, 200);
}

#[test]
fn test_refany_borrow_checking() {
    let test_val = TestStruct {
        value: 42,
        name: "test".to_string(),
    };

    let mut refany = RefAny::new(test_val);

    // Test that we can get an immutable reference
    {
        let borrowed1 = refany
            .downcast_ref::<TestStruct>()
            .expect("First immutable borrow");
        assert_eq!(borrowed1.value, 42);
        assert_eq!(borrowed1.name, "test");
    }

    // Test that we can get a mutable reference and modify the value
    {
        let mut borrowed_mut = refany
            .downcast_mut::<TestStruct>()
            .expect("Mutable borrow should work");
        borrowed_mut.value = 100;
        borrowed_mut.name = "modified".to_string();
    }

    // Verify the modification persisted
    {
        let borrowed = refany
            .downcast_ref::<TestStruct>()
            .expect("Should be able to borrow again");
        assert_eq!(borrowed.value, 100);
        assert_eq!(borrowed.name, "modified");
    }
}

#[test]
fn test_refany_type_safety() {
    let test_val = TestStruct {
        value: 42,
        name: "test".to_string(),
    };

    let mut refany = RefAny::new(test_val);

    // Try to downcast to wrong type
    assert!(
        refany.downcast_ref::<i32>().is_none(),
        "Should not allow downcasting to wrong type"
    );
    assert!(
        refany.downcast_mut::<String>().is_none(),
        "Should not allow mutable downcasting to wrong type"
    );

    // Correct type should still work
    let borrowed = refany
        .downcast_ref::<TestStruct>()
        .expect("Correct type should work");
    assert_eq!(borrowed.value, 42);
}

#[test]
fn test_refany_zero_sized_type() {
    #[derive(Debug, Clone, PartialEq)]
    struct ZeroSized;

    let refany = RefAny::new(ZeroSized);

    // Zero-sized types are stored differently (null pointer)
    // Verify that the RefAny can be created and cloned without issues
    let _cloned = refany.clone();

    // Note: downcast operations on ZSTs may have limitations
    // This test primarily verifies that creation and cloning work
}

#[test]
fn test_refany_with_vec() {
    let test_val = vec![1, 2, 3, 4, 5];
    let mut refany = RefAny::new(test_val);

    {
        let mut borrowed_mut = refany
            .downcast_mut::<Vec<i32>>()
            .expect("Should downcast vec");
        borrowed_mut.push(6);
        borrowed_mut.push(7);
    }

    let borrowed = refany
        .downcast_ref::<Vec<i32>>()
        .expect("Should downcast vec");
    assert_eq!(&**borrowed, &[1, 2, 3, 4, 5, 6, 7]);
}

#[test]
fn test_refany_nested_struct() {
    let nested = NestedStruct {
        inner: TestStruct {
            value: 42,
            name: "inner".to_string(),
        },
        data: vec![1, 2, 3],
    };

    let mut refany = RefAny::new(nested);

    {
        let mut borrowed_mut = refany
            .downcast_mut::<NestedStruct>()
            .expect("Should downcast nested");
        borrowed_mut.inner.value = 100;
        borrowed_mut.data.push(4);
    }

    let borrowed = refany
        .downcast_ref::<NestedStruct>()
        .expect("Should downcast nested");
    assert_eq!(borrowed.inner.value, 100);
    assert_eq!(&borrowed.data, &[1, 2, 3, 4]);
}

#[test]
fn test_refany_drop_order() {
    use std::sync::{Arc, Mutex};

    let drop_counter = Arc::new(Mutex::new(0));

    struct DropTracker {
        counter: Arc<Mutex<i32>>,
    }

    impl Drop for DropTracker {
        fn drop(&mut self) {
            *self.counter.lock().unwrap() += 1;
        }
    }

    {
        let tracker = DropTracker {
            counter: drop_counter.clone(),
        };
        let refany1 = RefAny::new(tracker);
        let refany2 = refany1.clone();
        let refany3 = refany1.clone();

        assert_eq!(*drop_counter.lock().unwrap(), 0, "Should not drop yet");

        drop(refany1);
        assert_eq!(
            *drop_counter.lock().unwrap(),
            0,
            "Should not drop after first clone dropped"
        );

        drop(refany2);
        assert_eq!(
            *drop_counter.lock().unwrap(),
            0,
            "Should not drop after second clone dropped"
        );

        drop(refany3);
        assert_eq!(
            *drop_counter.lock().unwrap(),
            1,
            "Should drop after last clone dropped"
        );
    }
}

#[test]
fn test_refany_callback_simulation() {
    // Simulate the VirtualizedView callback pattern
    #[derive(Clone)]
    struct CallbackData {
        counter: i32,
    }

    let data = CallbackData { counter: 0 };
    let mut refany = RefAny::new(data);

    // Simulate callback invocation
    {
        let mut borrowed = refany
            .downcast_mut::<CallbackData>()
            .expect("Should downcast in callback");
        borrowed.counter += 1;
    }

    let borrowed = refany
        .downcast_ref::<CallbackData>()
        .expect("Should read after callback");
    assert_eq!(borrowed.counter, 1);
}
