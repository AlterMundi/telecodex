//! Derived operational summaries with retained originals and atomic revision/cursor commits.
use super::*;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

pub const OBSERVER_POLICY: &str = "telecodex.observer/v1";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObserverCard {
    pub revision: i64,
    pub native: String,
    pub source_revision: String,
    pub boundary: Option<String>,
    pub policy: String,
    pub summary: String,
    pub supports: Vec<usize>,
    pub gap: bool,
    pub bounded: bool,
    pub observed_at: String,
    pub committed_at: String,
}

pub struct ObserverAttempt {
    pub id: String,
    pub source: Value,
    pub expected_revision: i64,
}

pub enum ObserverPreparation {
    Unchanged(ObserverCard),
    Retained(String),
    Ready(ObserverAttempt),
}

pub(super) fn binding(conn: &Connection, key: SessionKey, human: i64, native: &str) -> Result<i64> {
    conn.query_row("SELECT s.id FROM sessions s JOIN users u ON u.tg_user_id=s.creator_user_id WHERE s.chat_id=?1 AND s.thread_id=?2 AND s.creator_user_id=?3 AND u.allowed=1 AND s.codex_thread_id=?4",
        params![key.chat_id,key.thread_id,human,native],|r|r.get(0))
        .optional()?.context("Observer source is no longer an owned native binding")
}

pub(super) fn head(conn: &Connection, session: i64, native: &str) -> Result<Option<ObserverCard>> {
    let raw:Option<String> = conn.query_row("SELECT doc FROM interaction_observer_cards WHERE session_id=?1 AND native_thread=?2 ORDER BY revision DESC LIMIT 1",
        params![session,native],|r|r.get(0)).optional()?;
    raw.map(|raw| serde_json::from_str(&raw).map_err(Into::into))
        .transpose()
}

// Original observer evidence survives logical session removal, like the
// existing routed request ledger. Retained session IDs are never reassigned.
fn migrate_retained_observer_history(conn: &mut Connection) -> Result<()> {
    let mut stmt = conn.prepare("PRAGMA foreign_key_list(interaction_observer_attempts)")?;
    let session_fk = stmt
        .query_map([], |r| r.get::<_, String>(2))?
        .collect::<std::result::Result<Vec<_>, _>>()?
        .iter()
        .any(|table| table == "sessions");
    drop(stmt);
    if !session_fk {
        return Ok(());
    }
    let tx = conn.transaction()?;
    tx.execute_batch("CREATE TABLE interaction_observer_attempts_retained(
        id TEXT PRIMARY KEY,session_id INTEGER NOT NULL,human_id INTEGER NOT NULL,
        native_thread TEXT NOT NULL,source_revision TEXT NOT NULL,policy TEXT NOT NULL,
        expected_revision INTEGER NOT NULL,source_doc TEXT NOT NULL,observed_at TEXT NOT NULL,
        phase TEXT NOT NULL,instance_id TEXT NOT NULL);
        INSERT INTO interaction_observer_attempts_retained SELECT * FROM interaction_observer_attempts;
        CREATE TABLE interaction_observer_cards_retained(
        session_id INTEGER NOT NULL,native_thread TEXT NOT NULL,revision INTEGER NOT NULL,
        attempt_id TEXT NOT NULL UNIQUE REFERENCES interaction_observer_attempts_retained(id),
        doc TEXT NOT NULL,PRIMARY KEY(session_id,native_thread,revision));
        INSERT INTO interaction_observer_cards_retained SELECT * FROM interaction_observer_cards;
        DROP TABLE interaction_observer_cards;
        DROP TABLE interaction_observer_attempts;
        ALTER TABLE interaction_observer_attempts_retained RENAME TO interaction_observer_attempts;
        ALTER TABLE interaction_observer_cards_retained RENAME TO interaction_observer_cards;
        CREATE INDEX interaction_observer_source ON interaction_observer_attempts(session_id,native_thread,source_revision,policy);
        CREATE UNIQUE INDEX interaction_observer_active ON interaction_observer_attempts(session_id,native_thread,source_revision,policy) WHERE phase='summarizing';")?;
    tx.commit()?;
    Ok(())
}

impl Store {
    pub(super) fn init_observer_cards(&self) -> Result<()> {
        let mut conn = self.conn.lock().expect("store mutex poisoned");
        conn.execute_batch("CREATE TABLE IF NOT EXISTS interaction_observer_attempts(
            id TEXT PRIMARY KEY,session_id INTEGER NOT NULL,human_id INTEGER NOT NULL,
            native_thread TEXT NOT NULL,source_revision TEXT NOT NULL,policy TEXT NOT NULL,
            expected_revision INTEGER NOT NULL,source_doc TEXT NOT NULL,observed_at TEXT NOT NULL,
            phase TEXT NOT NULL,instance_id TEXT NOT NULL);
            CREATE INDEX IF NOT EXISTS interaction_observer_source
              ON interaction_observer_attempts(session_id,native_thread,source_revision,policy);
            CREATE UNIQUE INDEX IF NOT EXISTS interaction_observer_active
              ON interaction_observer_attempts(session_id,native_thread,source_revision,policy) WHERE phase='summarizing';
            CREATE TABLE IF NOT EXISTS interaction_observer_cards(
              session_id INTEGER NOT NULL,native_thread TEXT NOT NULL,
              revision INTEGER NOT NULL,attempt_id TEXT NOT NULL UNIQUE REFERENCES interaction_observer_attempts(id),
              doc TEXT NOT NULL,PRIMARY KEY(session_id,native_thread,revision));")?;
        migrate_retained_observer_history(&mut conn)?;
        conn.execute("UPDATE interaction_observer_attempts SET phase='uncertain' WHERE phase='summarizing' AND instance_id!=?",[&self.instance_id])?;
        Ok(())
    }

    pub fn observer_card(
        &self,
        key: SessionKey,
        human: i64,
        native: &str,
    ) -> Result<Option<ObserverCard>> {
        let conn = self.conn.lock().expect("store mutex poisoned");
        head(&conn, binding(&conn, key, human, native)?, native)
    }

    /// Freeze originals and reserve one synthesis before invoking a model. An
    /// unchanged source uses its retained card; uncertain calls require explicit retry.
    pub fn observer_prepare(
        &self,
        key: SessionKey,
        human: i64,
        source: &Value,
        expected: i64,
        retry: bool,
    ) -> Result<ObserverPreparation> {
        let native = source["native"]
            .as_str()
            .context("Observer snapshot has no native source")?;
        let revision = source["revision"]
            .as_str()
            .filter(|s| !s.is_empty() && s.len() <= 128)
            .context("Observer snapshot has no revision")?;
        let entries = source["entries"]
            .as_array()
            .context("Observer snapshot has no completed entries")?;
        let provisional = source["in_progress_entries"]
            .as_array()
            .context("Observer snapshot has no provisional entries")?;
        let turns = source["turn_ids"]
            .as_array()
            .context("Observer snapshot has no source turn references")?;
        let raw = serde_json::to_string(source)?;
        if entries.len() + provisional.len() > 120
            || raw.len() > 128_000
            || turns.len() != entries.len() + provisional.len()
            || entries.iter().chain(provisional).any(|v| !v.is_string())
            || turns.iter().any(|v| !v.is_string())
            || turns.iter().collect::<std::collections::HashSet<_>>().len() != turns.len()
        {
            return Err(anyhow!(
                "Observer snapshot exceeds source bounds or references"
            ));
        }
        let mut conn = self.conn.lock().expect("store mutex poisoned");
        let tx = conn.transaction()?;
        let session = binding(&tx, key, human, native)?;
        let previous = head(&tx, session, native)?;
        if let Some(card) = &previous
            && card.source_revision == revision
            && card.policy == OBSERVER_POLICY
        {
            let card = card.clone();
            observer_freshness::checked_on(
                &tx,
                session,
                native,
                revision,
                source["observer_activity_cut"].as_i64(),
                source["observer_checked_at"].as_str(),
                &self.instance_id,
            )?;
            tx.commit()?;
            return Ok(ObserverPreparation::Unchanged(card));
        }
        if previous.as_ref().map_or(0, |c| c.revision) != expected {
            return Err(anyhow!(
                "Observer revision changed; refresh from the retained head"
            ));
        }
        let prior:Option<String>=tx.query_row("SELECT phase FROM interaction_observer_attempts WHERE session_id=?1 AND native_thread=?2 AND source_revision=?3 AND policy=?4 ORDER BY rowid DESC LIMIT 1",
            params![session,native,revision,OBSERVER_POLICY],|r|r.get(0)).optional()?;
        if let Some(phase) = prior
            && !(phase == "uncertain" && retry)
        {
            return Ok(ObserverPreparation::Retained(phase));
        }
        let mut retained = source.clone();
        let old_source: Option<String> = if let Some(previous) = &previous {
            tx.query_row("SELECT a.source_doc FROM interaction_observer_cards c JOIN interaction_observer_attempts a ON a.id=c.attempt_id WHERE c.session_id=?1 AND c.native_thread=?2 AND c.revision=?3",params![session,native,previous.revision],|r|r.get(0)).optional()?
        } else {
            None
        };
        let old_source = old_source
            .map(|s| serde_json::from_str::<Value>(&s))
            .transpose()?;
        let old_entries = old_source
            .as_ref()
            .map(|s| {
                s["entries"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .chain(s["in_progress_entries"].as_array().into_iter().flatten())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let changed = entries
            .iter()
            .chain(provisional)
            .enumerate()
            .filter_map(|(n, e)| (!old_entries.contains(&e)).then_some(n))
            .collect::<Vec<_>>();
        retained["delta_indices"] = json!(changed);
        let raw = serde_json::to_string(&retained)?;
        let observed_at = source["observer_checked_at"]
            .as_str()
            .unwrap_or("not recorded")
            .to_owned();
        let id = Uuid::now_v7().to_string();
        tx.execute("INSERT INTO interaction_observer_attempts VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,'summarizing',?10)",
            params![id,session,human,native,revision,OBSERVER_POLICY,expected,raw,observed_at,self.instance_id])?;
        tx.commit()?;
        Ok(ObserverPreparation::Ready(ObserverAttempt {
            id,
            source: retained,
            expected_revision: expected,
        }))
    }

    pub fn observer_uncertain(&self, id: &str) -> Result<()> {
        self.conn.lock().expect("store mutex poisoned").execute("UPDATE interaction_observer_attempts SET phase='uncertain' WHERE id=?1 AND phase='summarizing' AND instance_id=?2",params![id,self.instance_id])?;
        Ok(())
    }

    /// Source/cursor, support selection and summary revision commit together.
    /// A stale model response never overwrites a newer retained card.
    pub fn observer_commit(
        &self,
        key: SessionKey,
        human: i64,
        attempt: &ObserverAttempt,
        summary: &str,
        supports: &[usize],
    ) -> Result<ObserverCard> {
        let source = &attempt.source;
        let native = source["native"]
            .as_str()
            .context("Observer native source missing")?;
        let count = source["entries"].as_array().map_or(0, Vec::len)
            + source["in_progress_entries"].as_array().map_or(0, Vec::len);
        if summary.trim().is_empty()
            || summary.len() > 16_000
            || supports.len() > 6
            || supports.iter().any(|n| *n >= count)
        {
            return Err(anyhow!(
                "Observer summary or support references exceed retained originals"
            ));
        }
        let mut conn = self.conn.lock().expect("store mutex poisoned");
        let tx = conn.transaction()?;
        let session = binding(&tx, key, human, native)?;
        let valid:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM interaction_observer_attempts WHERE id=?1 AND session_id=?2 AND human_id=?3 AND native_thread=?4 AND expected_revision=?5 AND source_doc=?6 AND phase='summarizing' AND instance_id=?7)",
            params![attempt.id,session,human,native,attempt.expected_revision,serde_json::to_string(source)?,self.instance_id],|r|r.get(0))?;
        if !valid
            || head(&tx, session, native)?
                .as_ref()
                .map_or(0, |c| c.revision)
                != attempt.expected_revision
        {
            return Err(anyhow!(
                "Observer attempt or expected revision changed; source remains retained"
            ));
        }
        let observed_at: String = tx.query_row(
            "SELECT observed_at FROM interaction_observer_attempts WHERE id=?1",
            [&attempt.id],
            |r| r.get(0),
        )?;
        let previous = head(&tx, session, native)?;
        let gap = previous.as_ref().is_some_and(|c| {
            c.gap
                || c.boundary.as_deref().is_some_and(|boundary| {
                    !source["turn_ids"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .any(|v| v.as_str() == Some(boundary))
                })
        });
        let card = ObserverCard {
            revision: attempt.expected_revision + 1,
            native: native.into(),
            source_revision: source["revision"]
                .as_str()
                .context("Observer source revision missing")?
                .into(),
            boundary: source["boundary"].as_str().map(str::to_owned),
            policy: OBSERVER_POLICY.into(),
            summary: summary.into(),
            supports: supports.to_vec(),
            gap,
            bounded: source["bounded"].as_bool().unwrap_or(true)
                || source["text_truncated"].as_bool().unwrap_or(true),
            observed_at,
            committed_at: now_string(),
        };
        tx.execute(
            "INSERT INTO interaction_observer_cards VALUES(?1,?2,?3,?4,?5)",
            params![
                session,
                native,
                card.revision,
                attempt.id,
                serde_json::to_string(&card)?
            ],
        )?;
        observer_freshness::checked_on(
            &tx,
            session,
            native,
            &card.source_revision,
            source["observer_activity_cut"].as_i64(),
            source["observer_checked_at"].as_str(),
            &self.instance_id,
        )?;
        tx.execute(
            "UPDATE interaction_observer_attempts SET phase='committed' WHERE id=?1",
            [&attempt.id],
        )?;
        tx.commit()?;
        Ok(card)
    }

    /// Support material always comes from the original retained source snapshot.
    pub fn observer_originals(
        &self,
        key: SessionKey,
        human: i64,
        native: &str,
        revision: i64,
    ) -> Result<Value> {
        let conn = self.conn.lock().expect("store mutex poisoned");
        let session = binding(&conn, key, human, native)?;
        let raw:String=conn.query_row("SELECT a.source_doc FROM interaction_observer_cards c JOIN interaction_observer_attempts a ON a.id=c.attempt_id WHERE c.session_id=?1 AND c.native_thread=?2 AND c.revision=?3",params![session,native,revision],|r|r.get(0))?;
        Ok(json!({"source":serde_json::from_str::<Value>(&raw)?}))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn source(revision: &str, turns: &[&str]) -> Value {
        json!({"native":"native-source","revision":revision,"boundary":turns.last(),
            "turn_ids":turns,"entries":turns.iter().map(|id|format!("[Native source turn {id}] original evidence")).collect::<Vec<_>>(),
            "in_progress_entries":[],"bounded":false,"text_truncated":false})
    }
    fn ready(
        store: &Store,
        key: SessionKey,
        source: &Value,
        expected: i64,
        retry: bool,
    ) -> ObserverAttempt {
        match store
            .observer_prepare(key, 100, source, expected, retry)
            .unwrap()
        {
            ObserverPreparation::Ready(a) => a,
            _ => panic!("expected one reserved synthesis"),
        }
    }
    #[test]
    fn cards_retain_original_deltas_and_atomically_advance_revision_and_cursor() {
        let file = tempfile::NamedTempFile::new().unwrap();
        let defaults = super::super::tests::defaults();
        let store = Store::open(file.path(), &[100, 101], &defaults).unwrap();
        let key = SessionKey::new(100, Some(7));
        store.ensure_session(key, 100, &defaults).unwrap();
        store
            .set_session_codex_thread(key, "native-source")
            .unwrap();
        let a = ready(&store, key, &source("one", &["turn-a"]), 0, false);
        assert_eq!(a.source["delta_indices"], json!([0]));
        assert!(matches!(
            store
                .observer_prepare(key, 100, &source("one", &["turn-a"]), 0, false)
                .unwrap(),
            ObserverPreparation::Retained(_)
        ));
        assert!(
            store
                .observer_commit(key, 100, &a, "bad source index", &[2])
                .is_err()
        );
        store.conn.lock().unwrap().execute_batch("CREATE TRIGGER abort_observer BEFORE INSERT ON interaction_observer_cards BEGIN SELECT RAISE(ABORT,'synthetic cursor failure'); END;").unwrap();
        assert!(
            store
                .observer_commit(key, 100, &a, "First supported interpretation", &[0])
                .is_err()
        );
        assert!(
            store
                .observer_card(key, 100, "native-source")
                .unwrap()
                .is_none()
        );
        let phase: String = store
            .conn
            .lock()
            .unwrap()
            .query_row(
                "SELECT phase FROM interaction_observer_attempts WHERE id=?",
                [&a.id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(phase, "summarizing");
        store
            .conn
            .lock()
            .unwrap()
            .execute_batch("DROP TRIGGER abort_observer")
            .unwrap();
        let card = store
            .observer_commit(key, 100, &a, "First supported interpretation", &[0])
            .unwrap();
        assert_eq!(card.boundary.as_deref(), Some("turn-a"));
        assert_eq!(card.revision, 1);
        assert!(matches!(
            store
                .observer_prepare(key, 100, &source("one", &["turn-a"]), 1, false)
                .unwrap(),
            ObserverPreparation::Unchanged(_)
        ));
        let b = ready(&store, key, &source("two", &["turn-a", "turn-b"]), 1, false);
        assert_eq!(b.source["delta_indices"], json!([1]));
        let c = ready(
            &store,
            key,
            &source("three", &["turn-a", "turn-b", "turn-c"]),
            1,
            false,
        );
        store
            .observer_commit(key, 100, &c, "Newer interpretation from originals", &[0, 2])
            .unwrap();
        assert!(
            store
                .observer_commit(key, 100, &b, "Late older interpretation", &[1])
                .is_err()
        );
        let card = store
            .observer_card(key, 100, "native-source")
            .unwrap()
            .unwrap();
        assert_eq!(card.source_revision, "three");
        assert_eq!(card.revision, 2);
        assert_eq!(
            store
                .observer_originals(key, 100, "native-source", 1)
                .unwrap()["source"]["entries"][0],
            source("one", &["turn-a"])["entries"][0]
        );
        let gap = ready(&store, key, &source("gap", &["turn-z"]), 2, false);
        assert!(
            store
                .observer_commit(
                    key,
                    100,
                    &gap,
                    "Bounded new window; missing intervening history",
                    &[0]
                )
                .unwrap()
                .gap
        );
        assert!(store.observer_card(key, 101, "native-source").is_err());
        store.set_session_codex_thread(key, "replacement").unwrap();
        assert!(store.observer_card(key, 100, "native-source").is_err());
        assert!(
            store
                .observer_card(key, 100, "replacement")
                .unwrap()
                .is_none()
        );
    }
    #[test]
    fn restart_retains_uncertain_synthesis_without_replay_and_explicit_retry_preserves_history() {
        let file = tempfile::NamedTempFile::new().unwrap();
        let defaults = super::super::tests::defaults();
        let key = SessionKey::new(100, Some(7));
        let old = {
            let store = Store::open(file.path(), &[100], &defaults).unwrap();
            store.ensure_session(key, 100, &defaults).unwrap();
            store
                .set_session_codex_thread(key, "native-source")
                .unwrap();
            ready(&store, key, &source("one", &["turn-a"]), 0, false)
        };
        let store = Store::open(file.path(), &[100], &defaults).unwrap();
        assert!(
            matches!(store.observer_prepare(key,100,&source("one",&["turn-a"]),0,false).unwrap(),ObserverPreparation::Retained(ref phase) if phase=="uncertain")
        );
        assert!(
            store
                .observer_commit(key, 100, &old, "Unconfirmed earlier response", &[0])
                .is_err()
        );
        let retry = ready(&store, key, &source("one", &["turn-a"]), 0, true);
        assert_ne!(old.id, retry.id);
        store
            .observer_commit(key, 100, &retry, "Explicit fresh interpretation", &[0])
            .unwrap();
        let count: i64 = store
            .conn
            .lock()
            .unwrap()
            .query_row(
                "SELECT COUNT(*) FROM interaction_observer_attempts",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 2);
    }
    #[test]
    fn legacy_session_links_migrate_without_losing_originals_or_blocking_session_removal() {
        let mut conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys=ON;
            CREATE TABLE sessions(id INTEGER PRIMARY KEY AUTOINCREMENT);
            INSERT INTO sessions VALUES(1);
            CREATE TABLE interaction_observer_attempts(id TEXT PRIMARY KEY,session_id INTEGER NOT NULL REFERENCES sessions(id),human_id INTEGER NOT NULL,native_thread TEXT NOT NULL,source_revision TEXT NOT NULL,policy TEXT NOT NULL,expected_revision INTEGER NOT NULL,source_doc TEXT NOT NULL,observed_at TEXT NOT NULL,phase TEXT NOT NULL,instance_id TEXT NOT NULL);
            CREATE TABLE interaction_observer_cards(session_id INTEGER NOT NULL REFERENCES sessions(id),native_thread TEXT NOT NULL,revision INTEGER NOT NULL,attempt_id TEXT NOT NULL UNIQUE REFERENCES interaction_observer_attempts(id),doc TEXT NOT NULL,PRIMARY KEY(session_id,native_thread,revision));
            INSERT INTO interaction_observer_attempts VALUES('a',1,100,'native-source','source-one','policy',0,'{\"original\":\"retained\"}','observed','committed','old-instance');
            INSERT INTO interaction_observer_cards VALUES(1,'native-source',1,'a','{\"summary\":\"retained interpretation\"}');").unwrap();
        assert!(conn.execute("DELETE FROM sessions WHERE id=1", []).is_err());
        migrate_retained_observer_history(&mut conn).unwrap();
        migrate_retained_observer_history(&mut conn).unwrap();
        conn.execute("DELETE FROM sessions WHERE id=1", []).unwrap();
        let original: String = conn
            .query_row(
                "SELECT source_doc FROM interaction_observer_attempts",
                [],
                |r| r.get(0),
            )
            .unwrap();
        let card: String = conn
            .query_row("SELECT doc FROM interaction_observer_cards", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(
            serde_json::from_str::<Value>(&original).unwrap()["original"],
            "retained"
        );
        assert_eq!(
            serde_json::from_str::<Value>(&card).unwrap()["summary"],
            "retained interpretation"
        );
        assert!(
            conn.prepare("PRAGMA foreign_key_check")
                .unwrap()
                .query([])
                .unwrap()
                .next()
                .unwrap()
                .is_none()
        );
    }
}
