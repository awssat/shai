#[cfg(test)]
mod tests {
    use crate::storage::Storage;
    use tempfile::tempdir;

    #[test]
    fn test_open_session_and_record_change() {
        let dir = tempdir().unwrap();
        let storage = Storage::open(dir.path());
        storage.init_schema();

        let session_key = "test-session";
        let prompt = "Initial prompt";
        let llm = "claude";

        storage.open_session(session_key, prompt, llm, None);

        let file_path = "src/main.rs";
        let content = b"fn main() {}";
        let tool_name = "Write";
        let query = "";

        storage.record_change(session_key, llm, file_path, content, crate::storage::ChangeHints { tool_name, query_str: query, payload_json: None });

        let history = storage.get_history(10);
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].prompt, prompt);
        assert_eq!(history[0].changes.len(), 1);
        assert_eq!(history[0].changes[0].file_path, file_path);
    }

    #[test]
    fn test_checkpoint_clears_missing_checkpoint_requirement() {
        let dir = tempdir().unwrap();
        let storage = Storage::open(dir.path());
        storage.init_schema();

        let session_key = "checkpoint-session";
        let llm = "claude";
        storage.open_session(session_key, "Implement feature", llm, None);
        storage.record_change(
            session_key,
            llm,
            "src/main.rs",
            b"fn main() {}",
            crate::storage::ChangeHints { tool_name: "Write", query_str: "", payload_json: None },
        );

        assert!(storage.session_missing_checkpoint(session_key, llm));

        let checkpoint_id = storage
            .record_checkpoint(session_key, llm, "implemented main")
            .unwrap();
        assert!(storage.event_exists(checkpoint_id, "checkpoint_created"));
        assert!(!storage.session_missing_checkpoint(session_key, llm));
    }

    #[test]
    fn test_guard_snapshots_do_not_require_checkpoint() {
        let dir = tempdir().unwrap();
        let storage = Storage::open(dir.path());
        storage.init_schema();

        let session_key = "guard-session";
        let llm = "claude";
        storage.open_session(session_key, "Avoid risky restore", llm, None);
        storage.record_guard_snapshot(
            session_key,
            llm,
            "src/main.rs",
            b"fn main() {}",
            "guard snapshot before destructive shell command: git checkout -- src/main.rs",
            None,
        );

        assert!(!storage.session_missing_checkpoint(session_key, llm));
    }

    #[test]
    fn test_export_import_round_trip_preserves_events() {
        let source = tempdir().unwrap();
        let target = tempdir().unwrap();

        let source_storage = Storage::open(source.path());
        source_storage.init_schema();
        source_storage.open_session("roundtrip", "Do work", "claude", None);
        source_storage.record_change(
            "roundtrip",
            "claude",
            "src/lib.rs",
            b"pub fn value() -> i32 { 1 }",
            crate::storage::ChangeHints { tool_name: "Write", query_str: "", payload_json: None },
        );
        let checkpoint_id = source_storage
            .record_checkpoint("roundtrip", "claude", "finished value")
            .unwrap();
        assert!(source_storage.event_exists(checkpoint_id, "checkpoint_created"));
        source_storage
            .add_memory_fact("build", "cargo test --quiet", true, "manual", Some("main"))
            .unwrap();
        source_storage
            .add_memory_decision(
                "Use timeline events",
                "Single canonical replay stream",
                "Keep sessions/changes split",
                "active",
                true,
                Some("main"),
            )
            .unwrap();

        let mut exported = Vec::new();
        let export_stats = source_storage.export_to(&mut exported).unwrap();
        assert_eq!(export_stats.sessions, 1);
        assert!(export_stats.events >= 3);
        assert!(export_stats.memory_records >= 2);

        let target_storage = Storage::open(target.path());
        target_storage.init_schema();
        let import_stats = target_storage.import_from(exported.as_slice()).unwrap();
        assert_eq!(import_stats.sessions_inserted, 1);
        assert!(import_stats.events_inserted >= 3);
        assert!(import_stats.memory_records_inserted >= 2);
        assert_eq!(target_storage.get_history(10).len(), 1);
        let memory_summary = target_storage.ranked_memory_summary(Some("main"), 10);
        assert!(memory_summary
            .iter()
            .any(|line| line.contains("build = cargo test --quiet")));
        assert!(memory_summary
            .iter()
            .any(|line| line.contains("Use timeline events")));
    }

    #[test]
    fn test_project_timeline_returns_canonical_events() {
        let dir = tempdir().unwrap();
        let storage = Storage::open(dir.path());
        storage.init_schema();
        storage.open_session("timeline-session", "Implement timeline", "claude", None);
        storage
            .record_checkpoint("timeline-session", "claude", "timeline ready")
            .unwrap();

        let timeline = storage.get_project_timeline(10);
        assert!(!timeline.is_empty());
        assert_eq!(timeline[0].session_key, "timeline-session");
        assert!(timeline
            .iter()
            .any(|row| row.event.event_kind == "checkpoint_created"));
    }

    #[test]
    fn test_memory_verification_promotes_existing_records() {
        let dir = tempdir().unwrap();
        let storage = Storage::open(dir.path());
        storage.init_schema();

        let fact_id = storage
            .add_memory_fact("build", "cargo test --quiet", false, "manual", None)
            .unwrap();
        let decision_id = storage
            .add_memory_decision(
                "Use timeline events",
                "Single canonical stream",
                "",
                "active",
                false,
                None,
            )
            .unwrap();

        assert_eq!(
            storage.verify_memory_fact(fact_id).unwrap(),
            crate::storage::MemoryVerifyOutcome::Verified
        );
        assert_eq!(
            storage.verify_memory_decision(decision_id).unwrap(),
            crate::storage::MemoryVerifyOutcome::Verified
        );
        assert_eq!(
            storage.verify_memory_fact(fact_id).unwrap(),
            crate::storage::MemoryVerifyOutcome::AlreadyVerified
        );
        assert_eq!(
            storage.verify_memory_decision(decision_id).unwrap(),
            crate::storage::MemoryVerifyOutcome::AlreadyVerified
        );

        let facts = storage.list_memory_facts(10);
        let decisions = storage.list_memory_decisions(10);
        assert!(facts.iter().any(|fact| fact.id == fact_id && fact.verified));
        assert!(decisions
            .iter()
            .any(|decision| decision.id == decision_id && decision.verified));
    }

    #[test]
    fn test_project_id_stability() {
        let dir = tempdir().unwrap();
        let storage = Storage::open(dir.path());
        let id1 = storage.get_project_id();
        let id2 = storage.get_project_id();
        assert_eq!(id1, id2);
    }

    #[test]
    fn test_gc_logic() {
        let dir = tempdir().unwrap();
        let storage = Storage::open(dir.path());
        storage.init_schema();
        let result = storage.gc(30, true, false);
        assert_eq!(result.blob_count, 0);
    }
}
