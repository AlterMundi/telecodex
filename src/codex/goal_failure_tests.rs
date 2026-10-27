use super::*;

fn blocked_goal() -> NativeGoal {
    serde_json::from_value(json!({"threadId":"native","objective":"Retained task",
        "status":"blocked","tokenBudget":100,"tokensUsed":42,"timeUsedSeconds":3,
        "createdAt":1,"updatedAt":2}))
    .unwrap()
}

#[test]
fn goal_block_preserves_explicit_causal_turn_without_inventing_one() {
    let event = blocked_goal_observation(&json!({"turnId":"failed-turn"}), blocked_goal());
    assert!(
        matches!(event,Some(CodexEvent::NativeGoalBlocked {turn_id,goal})
        if turn_id=="failed-turn" && goal.thread_id=="native" && goal.tokens_used==42)
    );
    for missing in [json!({}), json!({"turnId":null}), json!({"turnId":""})] {
        assert!(blocked_goal_observation(&missing, blocked_goal()).is_none());
    }
    let mut paused = blocked_goal();
    paused.status = "paused".into();
    assert!(blocked_goal_observation(&json!({"turnId":"a-turn"}), paused).is_none());
}

#[tokio::test]
async fn terminal_error_preserves_native_identity_and_structured_cause() {
    let mut events = vec![];
    let mut summary = RunSummary {
        codex_thread_id: Some("native".into()),
        assistant_text: String::new(),
        stderr_text: String::new(),
        draft_item: None,
    };
    let mut active = Some("failed-turn".into());
    let mut failure = None;
    let mut completed = false;
    let mut message_completed = false;
    let error =
        json!({"codexErrorInfo":"serverOverloaded","message":"Selected model is at capacity"});
    handle_notification(
        "turn/completed",
        &json!({"threadId":"native","turn":{
        "id":"failed-turn","status":"failed","error":error}}),
        &mut summary,
        &mut active,
        &mut failure,
        &mut completed,
        &mut message_completed,
        &mut |event| {
            events.push(event);
            async { Ok(CodexEventOutcome::None) }
        },
    )
    .await
    .unwrap();
    assert_eq!(events.len(), 2);
    assert!(
        matches!(&events[1],CodexEvent::NativeTurnTerminal {id,status}
        if id=="failed-turn" && status=="failed")
    );
    assert!(
        matches!(&events[0],CodexEvent::NativeTurnFailure {thread_id,id,error:observed}
        if thread_id=="native" && id=="failed-turn" && observed==&error)
    );
    assert!(completed && active.is_none());
    assert_eq!(failure.as_deref(), Some("Selected model is at capacity"));
}

#[tokio::test]
async fn completion_with_error_shaped_data_is_not_a_terminal_failure() {
    let mut events = vec![];
    let mut summary = RunSummary {
        codex_thread_id: Some("native".into()),
        assistant_text: String::new(),
        stderr_text: String::new(),
        draft_item: None,
    };
    let mut active = Some("done".into());
    let mut failure = None;
    let mut completed = false;
    let mut message_completed = false;
    handle_notification(
        "turn/completed",
        &json!({"threadId":"native","turn":{
        "id":"done","status":"completed","error":{"codexErrorInfo":"serverOverloaded"}}}),
        &mut summary,
        &mut active,
        &mut failure,
        &mut completed,
        &mut message_completed,
        &mut |event| {
            events.push(event);
            async { Ok(CodexEventOutcome::None) }
        },
    )
    .await
    .unwrap();
    assert_eq!(events.len(), 1);
    assert!(failure.is_none());
    assert!(
        !events
            .iter()
            .any(|e| matches!(e, CodexEvent::NativeTurnFailure { .. }))
    );
}
