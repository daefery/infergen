//! Placeholder handlers for commands that land in later epics.

/// Print a not-implemented notice naming the owning epic, then succeed.
///
/// Exit 0 keeps the scaffold friendly; the message makes the status explicit
/// so behavior is never silently wrong.
pub fn not_implemented(command: &str, epic: &str) -> anyhow::Result<()> {
    println!("{command}: not yet implemented — lands in {epic}");
    Ok(())
}
