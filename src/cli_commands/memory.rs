use super::shared::{get_current_git_branch, local_shai_or_die};
use crate::storage::MemoryVerifyOutcome;

fn format_scope(branch_refs: &[String]) -> String {
    if branch_refs.is_empty() {
        "global".to_string()
    } else {
        format!("branch:{}", branch_refs.join(","))
    }
}

pub(crate) fn cmd_memory_add_fact(
    key: &str,
    content: &str,
    source: &str,
    verified: bool,
    global: bool,
) {
    let (shai_dir, db) = local_shai_or_die();
    let branch = if global {
        None
    } else {
        get_current_git_branch(shai_dir.parent().unwrap_or(&shai_dir))
    };

    match db.add_memory_fact(key, content, verified, source, branch.as_deref()) {
        Ok(id) => println!("✅ memory fact recorded [{}] {}", id, key),
        Err(err) => eprintln!("❌ Failed to record memory fact: {}", err),
    }
}

pub(crate) fn cmd_memory_add_decision(
    title: &str,
    rationale: &str,
    alternatives: &str,
    status: &str,
    verified: bool,
    global: bool,
) {
    let (shai_dir, db) = local_shai_or_die();
    let branch = if global {
        None
    } else {
        get_current_git_branch(shai_dir.parent().unwrap_or(&shai_dir))
    };

    match db.add_memory_decision(
        title,
        rationale,
        alternatives,
        status,
        verified,
        branch.as_deref(),
    ) {
        Ok(id) => println!("✅ memory decision recorded [{}] {}", id, title),
        Err(err) => eprintln!("❌ Failed to record memory decision: {}", err),
    }
}

pub(crate) fn cmd_memory_list(limit: u32) {
    let (shai_dir, db) = local_shai_or_die();
    let branch = get_current_git_branch(shai_dir.parent().unwrap_or(&shai_dir));
    let ranked = db.ranked_memory_summary(branch.as_deref(), limit);
    let facts = db.list_memory_facts(limit);
    let decisions = db.list_memory_decisions(limit);

    println!("shai memory\n");
    if let Some(branch) = &branch {
        println!("  active branch      {}", branch);
    }

    if ranked.is_empty() && facts.is_empty() && decisions.is_empty() {
        println!("  no durable memory recorded");
        return;
    }

    if !ranked.is_empty() {
        println!("  ranked context:");
        for line in ranked {
            println!("    - {}", line);
        }
    }

    if !facts.is_empty() {
        println!("\n  facts:");
        for fact in facts {
            let badge = if fact.verified {
                "verified"
            } else {
                "unverified"
            };
            println!(
                "    [fact:{}][{}][{}] {} = {}",
                fact.id,
                badge,
                format_scope(&fact.branch_refs),
                fact.fact_key,
                fact.content
            );
        }
    }

    if !decisions.is_empty() {
        println!("\n  decisions:");
        for decision in decisions {
            let badge = if decision.verified {
                "verified"
            } else {
                "unverified"
            };
            let rationale = if decision.rationale.trim().is_empty() {
                "".to_string()
            } else {
                format!(" — {}", decision.rationale)
            };
            println!(
                "    [decision:{}][{}][{}] {} ({}){}",
                decision.id,
                badge,
                format_scope(&decision.branch_refs),
                decision.title,
                decision.status,
                rationale
            );
        }
    }
}

pub(crate) fn cmd_memory_verify_fact(id: i64) {
    let (_shai_dir, db) = local_shai_or_die();
    match db.verify_memory_fact(id) {
        Ok(MemoryVerifyOutcome::Verified) => {
            println!("✅ memory fact verified [{}]", id);
        }
        Ok(MemoryVerifyOutcome::AlreadyVerified) => {
            println!("ℹ️ memory fact already verified [{}]", id);
        }
        Ok(MemoryVerifyOutcome::NotFound) => {
            eprintln!("❌ Memory fact not found [{}]", id);
        }
        Err(err) => eprintln!("❌ Failed to verify memory fact: {}", err),
    }
}

pub(crate) fn cmd_memory_verify_decision(id: i64) {
    let (_shai_dir, db) = local_shai_or_die();
    match db.verify_memory_decision(id) {
        Ok(MemoryVerifyOutcome::Verified) => {
            println!("✅ memory decision verified [{}]", id);
        }
        Ok(MemoryVerifyOutcome::AlreadyVerified) => {
            println!("ℹ️ memory decision already verified [{}]", id);
        }
        Ok(MemoryVerifyOutcome::NotFound) => {
            eprintln!("❌ Memory decision not found [{}]", id);
        }
        Err(err) => eprintln!("❌ Failed to verify memory decision: {}", err),
    }
}
