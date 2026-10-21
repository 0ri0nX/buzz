// Compile the exact production contract without starting Tauri or any runtime.
#[path = "../../src/managed_agents/owner_test_saved_developer_core.rs"]
#[cfg(test)]
mod contract;

#[path = "../../src/commands/owner_test_claude_create_core.rs"]
#[cfg(test)]
mod claude_create_contract;

#[path = "../../src/commands/owner_test_claude_lifecycle_core.rs"]
#[cfg(test)]
mod claude_lifecycle_contract;
