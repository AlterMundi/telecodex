//! Telegram topic facts are distinct from conversation/native titles and activity.
use super::*;
use serde_json::{Value, json};

fn state_key(key: SessionKey) -> String {
    format!("topic_catalog:{}:{}", key.chat_id, key.thread_id)
}

impl Store {
    /// Merge observed fields independently: delayed creation must not overwrite a
    /// newer rename/close, and a close without a name must not invent one.
    pub fn observe_topic(
        &self,
        key: SessionKey,
        name: Option<&str>,
        icon: Option<Value>,
        lifecycle: Option<&str>,
        source: Value,
    ) -> Result<bool> {
        if key.thread_id <= 0
            || !matches!(
                source["kind"].as_str(),
                Some(
                    "telegram_update"
                        | "create_receipt"
                        | "rename_receipt"
                        | "lifecycle_receipt"
                        | "icon_receipt"
                )
            )
            || name.is_some_and(|s| {
                s.is_empty() || s.chars().count() > 128 || s.chars().any(char::is_control)
            })
            || lifecycle.is_some_and(|s| !matches!(s, "open" | "closed" | "deleted"))
        {
            return Err(anyhow!("Invalid observed topic metadata"));
        }
        let mut conn = self.conn.lock().expect("store mutex poisoned");
        let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let old: Option<String> = tx
            .query_row(
                "SELECT value FROM bot_state WHERE key=?",
                [state_key(key)],
                |r| r.get(0),
            )
            .optional()?;
        let mut doc = old
            .map(|s| serde_json::from_str::<Value>(&s))
            .transpose()?
            .unwrap_or_else(|| json!({"schema":"telecodex.topic-catalog/v1","key":key}));
        let mut name_updated = false;
        let mut fields = vec![
            ("name", name.map(|s| json!(s))),
            ("lifecycle", lifecycle.map(|s| json!(s))),
        ];
        if let Some(icon) = icon.as_ref() {
            for (field, key) in [
                ("icon_color", "color"),
                ("icon_custom_emoji_id", "custom_emoji_id"),
            ] {
                fields.push((field, icon.get(key).filter(|v| !v.is_null()).cloned()));
            }
        }
        for (field, value) in fields {
            let Some(value) = value else { continue };
            let existing = &doc[field];
            let previous_order = existing["update_watermark"]
                .as_i64()
                .or_else(|| existing["source"]["update_id"].as_i64());
            let incoming_order = source["update_id"].as_i64();
            if !existing.is_null()
                && (source["kind"] == "create_receipt"
                    || matches!((previous_order,incoming_order),(Some(a),Some(b)) if a>=b))
            {
                continue;
            }
            if field == "name" {
                name_updated = true;
            }
            doc[field] = json!({"value":value,"source":source,"update_watermark":incoming_order.or(previous_order),"observed_at":Utc::now().to_rfc3339()});
        }
        tx.execute("INSERT INTO bot_state(key,value) VALUES(?1,?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value",params![state_key(key),doc.to_string()])?;
        tx.commit()?;
        Ok(name_updated)
    }
    /// Finite, owner-local inventory. Legacy summaries remain unverified; reads,
    /// polls and metadata updates never count as conversational activity.
    pub fn topic_inventory(&self, human: i64, days: i64) -> Result<Value> {
        if !(1..=365).contains(&days) {
            return Err(anyhow!("Choose 1–365 activity days"));
        }
        let conn = self.conn.lock().expect("store mutex poisoned");
        let mut query=conn.prepare("SELECT s.id,s.chat_id,s.thread_id,s.session_title,s.codex_thread_id,s.force_fresh_thread,s.busy,b.value,
            (SELECT MAX(t.started_at) FROM turns t WHERE t.session_id=s.id AND t.from_user_id=?1)
            FROM sessions s JOIN users u ON u.tg_user_id=s.creator_user_id
            LEFT JOIN bot_state b ON b.key='topic_catalog:'||s.chat_id||':'||s.thread_id
            WHERE s.creator_user_id=?1 AND u.allowed=1 ORDER BY s.id DESC LIMIT 513")?;
        let rows = query
            .query_map([human], |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, i64>(1)?,
                    r.get::<_, i64>(2)?,
                    r.get::<_, Option<String>>(3)?,
                    r.get::<_, Option<String>>(4)?,
                    r.get::<_, bool>(5)?,
                    r.get::<_, bool>(6)?,
                    r.get::<_, Option<String>>(7)?,
                    r.get::<_, Option<String>>(8)?,
                ))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let mut items = Vec::new();
        for (session, chat, topic, summary, native, fresh, busy, catalog, last) in
            rows.iter().take(512)
        {
            let observed = catalog
                .as_ref()
                .map(|s| serde_json::from_str::<Value>(s))
                .transpose()?;
            items.push(json!({"session":session,"key":{"chat_id":chat,"thread_id":topic},"native":native,"native_binding_current":native.is_some() && !fresh,
                "title_summary":summary,"telegram":observed,"lifecycle":observed.as_ref().map(|v|&v["lifecycle"]["value"]).unwrap_or(&Value::Null),
                "bridge_busy":busy,"last_bridge_conversation_at":last,"activity_days":days,
                "recent_bridge_conversation":last.as_ref().and_then(|s|chrono::DateTime::parse_from_rfc3339(s).ok()).is_some_and(|t|t>=Utc::now()-Duration::days(days)),
                "native_execution":"unobserved","goal_state":"unobserved",
                "goal_recovery":if !fresh {native.as_ref().map(|id|super::goal_retries::compact_view_on(&conn,human,SessionKey::new(*chat,Some(*topic)),*session,id)).transpose()?}else{None}}));
        }
        Ok(
            json!({"schema":"telecodex.topic-inventory/v1","observed_at":Utc::now().to_rfc3339(),"items":items,"truncated":rows.len()>512,
            "activity_scope":"bridge_attributed_turns","meaning":"Telegram fields retain dated provenance; absence means unknown. Bridge busy and session summaries do not certify native execution, goal state or a live Telegram title."}),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn topic_catalog_preserves_field_chronology_icons_unknowns_and_restart() {
        let file = tempfile::NamedTempFile::new().unwrap();
        let defaults = super::super::tests::defaults();
        let store = Store::open(file.path(), &[100, 101], &defaults).unwrap();
        let key = SessionKey::new(100, Some(7));
        let legacy = SessionKey::new(100, Some(8));
        store.ensure_session(key, 100, &defaults).unwrap();
        store.ensure_session(legacy, 100, &defaults).unwrap();
        store
            .set_session_title(legacy, Some("Unverified summary"))
            .unwrap();
        let event = |id| json!({"kind":"telegram_update","update_id":id});
        assert!(
            store
                .observe_topic(
                    key,
                    Some("Human name"),
                    Some(json!({"color":123,"custom_emoji_id":"one"})),
                    Some("open"),
                    event(10)
                )
                .unwrap()
        );
        store
            .observe_topic(key, None, None, Some("closed"), event(15))
            .unwrap();
        // The name and icon can advance independently of the later close.
        assert!(
            store
                .observe_topic(
                    key,
                    Some("New name"),
                    Some(json!({"custom_emoji_id":""})),
                    None,
                    event(14)
                )
                .unwrap()
        );
        assert!(
            !store
                .observe_topic(key, Some("Old name"), None, Some("open"), event(11))
                .unwrap()
        );
        store
            .observe_topic(
                key,
                Some("Delayed API creation"),
                None,
                Some("open"),
                json!({"kind":"create_receipt"}),
            )
            .unwrap();
        let inventory = store.topic_inventory(100, 7).unwrap();
        let rows = inventory["items"].as_array().unwrap();
        let topic = rows.iter().find(|v| v["key"]["thread_id"] == 7).unwrap();
        assert_eq!(topic["telegram"]["name"]["value"], "New name");
        assert_eq!(topic["lifecycle"], "closed");
        assert_eq!(topic["telegram"]["icon_color"]["value"], 123);
        assert_eq!(topic["telegram"]["icon_custom_emoji_id"]["value"], "");
        store
            .observe_topic(
                key,
                None,
                Some(json!({"custom_emoji_id":"receipt"})),
                None,
                json!({"kind":"icon_receipt"}),
            )
            .unwrap();
        store
            .observe_topic(
                key,
                None,
                Some(json!({"custom_emoji_id":"old"})),
                None,
                event(13),
            )
            .unwrap();
        let snapshot = store.topic_inventory(100, 7).unwrap();
        assert_eq!(
            snapshot["items"]
                .as_array()
                .unwrap()
                .iter()
                .find(|v| v["key"]["thread_id"] == 7)
                .unwrap()["telegram"]["icon_custom_emoji_id"]["value"],
            "receipt"
        );

        assert_eq!(topic["recent_bridge_conversation"], false);
        assert_eq!(topic["goal_state"], "unobserved");
        let unknown = rows.iter().find(|v| v["key"]["thread_id"] == 8).unwrap();
        assert_eq!(unknown["title_summary"], "Unverified summary");
        assert!(unknown["telegram"].is_null() && unknown["lifecycle"].is_null());
        assert!(
            store.topic_inventory(101, 7).unwrap()["items"]
                .as_array()
                .unwrap()
                .is_empty()
        );
        assert!(store.topic_inventory(100, 0).is_err());
        drop(store);
        let store = Store::open(file.path(), &[100, 101], &defaults).unwrap();
        assert_eq!(
            store.topic_inventory(100, 7).unwrap()["items"],
            snapshot["items"]
        );
    }
}
