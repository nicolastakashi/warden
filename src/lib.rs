//! Warden — a deterministic, agent-agnostic policy engine for AI-agent code.
//!
//! One rule format, two consumers: a CI gate (scores a path, blocks on `block`
//! rules) and a runtime gate (block/allow on one proposed agent action). The
//! core only ever sees `CodeUnit` in and `Violation` out; the only
//! agent-specific surface is `adapters::claude_code`.

pub mod adapters;
pub mod ci_gate;
pub mod glob;
pub mod lang;
pub mod load;
pub mod matchers;
pub mod report;
pub mod results;
pub mod runtime_gate;
pub mod schema;
pub mod score;
