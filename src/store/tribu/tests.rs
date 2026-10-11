use super::*;
use std::sync::Arc;

fn open(path: &Path) -> Store {
    Store::open(
        path,
        &[1],
        &SessionDefaults {
            cwd: path.parent().unwrap().to_path_buf(),
            model: None,
            reasoning_effort: None,
            session_prompt: None,
            sandbox_mode: "read-only".into(),
            approval_policy: "never".into(),
            search_mode: crate::config::SearchMode::Disabled,
            add_dirs: vec![],
        },
    )
    .unwrap()
}

#[test]
fn concurrent_duplicates_have_one_admission_and_pause_withdraws_queue() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("store.sqlite");
    let store = Arc::new(open(&path));
    let key = SessionKey::new(1, Some(2));
    let handles = (0..8)
        .map(|_| {
            let store = store.clone();
            std::thread::spawn(move || {
                store
                    .tribu_admit(
                        key,
                        "tribu:source",
                        Some("source"),
                        &json!({"id":"source"}),
                        2,
                    )
                    .unwrap()
                    .fresh
            })
        })
        .collect::<Vec<_>>();
    assert_eq!(
        handles
            .into_iter()
            .map(|h| h.join().unwrap())
            .filter(|fresh| *fresh)
            .count(),
        1
    );
    assert!(
        store
            .tribu_admit(key, "private:2", None, &json!({"private":true}), 2)
            .unwrap()
            .fresh
    );
    assert_eq!(
        store
            .tribu_admit(key, "private:3", None, &json!({"private":true}), 2)
            .unwrap()
            .state,
        "retained-overflow"
    );
    store.tribu_pause(key, true).unwrap();
    assert!(!store.tribu_reserve(key, "tribu:source").unwrap());
    assert_eq!(
        store.tribu_admission("private:2").unwrap().unwrap().state,
        "withdrawn-paused"
    );
    drop(store);
    let reopened = open(&path);
    assert_eq!(reopened.tribu_status(key).unwrap()["paused"], true);
}

#[test]
fn possible_submission_survives_restart_and_terminal_reconciliation_never_replays() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("store.sqlite");
    let store = open(&path);
    let key = SessionKey::new(1, Some(2));
    store
        .tribu_admit(key, "one", Some("s"), &json!({}), 20)
        .unwrap();
    assert!(store.tribu_reserve(key, "one").unwrap());
    store.tribu_link_turn("one", 1).unwrap();
    store
        .tribu_native_thread(key, "one", "native-thread")
        .unwrap();
    store.tribu_submitting(key, "one").unwrap();
    store.tribu_native_turn("one", "native-turn", None).unwrap();
    store.tribu_admit(key, "two", None, &json!({}), 20).unwrap();
    drop(store);
    let store = open(&path);
    assert_eq!(
        store.tribu_admission("one").unwrap().unwrap().state,
        "undetermined"
    );
    assert_eq!(
        store.tribu_admission("two").unwrap().unwrap().state,
        "withdrawn-restart"
    );
    assert_eq!(
        store
            .tribu_admit(key, "three", None, &json!({}), 20)
            .unwrap()
            .state,
        "blocked-uncertain"
    );
    assert!(
        !store
            .tribu_admit(key, "one", Some("s"), &json!({}), 20)
            .unwrap()
            .fresh
    );
    store
        .tribu_reconcile("one", "native-turn", "inProgress")
        .unwrap();
    assert_eq!(store.tribu_status(key).unwrap()["undetermined_count"], 1);
    assert!(
        store
            .tribu_reconcile("one", "different-turn", "completed")
            .is_err()
    );
    store
        .tribu_reconcile("one", "native-turn", "interrupted")
        .unwrap();
    assert_eq!(store.tribu_status(key).unwrap()["undetermined_count"], 0);
    assert!(!store.tribu_reserve(key, "one").unwrap());
    let other = store
        .tribu_admit(key, "four", None, &json!({}), 20)
        .unwrap();
    assert_eq!(other.state, "queued");
}

#[test]
fn no_submission_error_is_not_an_unknown_model_and_pause_blocks_prepared_work() {
    let root = tempfile::tempdir().unwrap();
    let store = open(&root.path().join("store.sqlite"));
    let key = SessionKey::new(1, Some(2));
    store.tribu_admit(key, "one", None, &json!({}), 20).unwrap();
    assert!(store.tribu_reserve(key, "one").unwrap());
    store.tribu_link_turn("one", 1).unwrap();
    store.tribu_pause(key, true).unwrap();
    assert!(store.tribu_submitting(key, "one").is_err());
    store.tribu_finish("one").unwrap();
    assert_eq!(
        store.tribu_admission("one").unwrap().unwrap().state,
        "not-submitted"
    );
}

#[test]
fn recorded_terminal_survives_crash_before_presentation_finish() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("store.sqlite");
    let store = open(&path);
    let key = SessionKey::new(1, Some(2));
    store.tribu_admit(key, "one", None, &json!({}), 20).unwrap();
    assert!(store.tribu_reserve(key, "one").unwrap());
    store.tribu_link_turn("one", 1).unwrap();
    store.tribu_submitting(key, "one").unwrap();
    store
        .tribu_native_turn("one", "native-turn", Some("completed"))
        .unwrap();
    drop(store);
    let store = open(&path);
    assert_eq!(
        store.tribu_admission("one").unwrap().unwrap().state,
        "completed"
    );
    assert_eq!(store.tribu_status(key).unwrap()["undetermined_count"], 0);
    assert!(
        !store
            .tribu_admit(key, "one", None, &json!({}), 20)
            .unwrap()
            .fresh
    );
}

#[test]
fn overflow_does_not_hide_active_turn_from_pause_controls() {
    let root = tempfile::tempdir().unwrap();
    let store = open(&root.path().join("store.sqlite"));
    let key = SessionKey::new(1, Some(2));
    store
        .tribu_admit(key, "active", None, &json!({}), 20)
        .unwrap();
    assert!(store.tribu_reserve(key, "active").unwrap());
    store.tribu_link_turn("active", 1).unwrap();
    for n in 0..50 {
        store
            .tribu_admit(key, &format!("queued-{n}"), None, &json!({}), 20)
            .unwrap();
    }
    store.tribu_pause(key, true).unwrap();
    let controls = store.tribu_controllable(key, true).unwrap();
    assert_eq!(controls.len(), 1);
    assert_eq!(controls[0]["id"], "active");
}
