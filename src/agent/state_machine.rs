//! Agent State Machine
//!
//! Manages the trading lifecycle states:
//! - Idle: Waiting for opportunities
//! - Opening: Executing entry trade
//! - Monitoring: Watching position
//! - Closing: Executing exit trade
//! - Rebalancing: Adjusting hedge
//! - Paused: Risk-triggered halt
//! - Error: Recovery state

use std::time::Instant;
use tracing::{info, debug, warn};

/// Agent states
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AgentState {
    /// Idle - waiting for trade opportunities
    Idle,
    /// Opening a new position
    Opening,
    /// Monitoring an active position
    Monitoring,
    /// Closing a position
    Closing,
    /// Rebalancing hedge
    Rebalancing,
    /// Paused due to risk conditions
    Paused,
    /// Error state requiring recovery
    Error,
}

impl std::fmt::Display for AgentState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AgentState::Idle => write!(f, "Idle"),
            AgentState::Opening => write!(f, "Opening"),
            AgentState::Monitoring => write!(f, "Monitoring"),
            AgentState::Closing => write!(f, "Closing"),
            AgentState::Rebalancing => write!(f, "Rebalancing"),
            AgentState::Paused => write!(f, "Paused"),
            AgentState::Error => write!(f, "Error"),
        }
    }
}

/// State transition record
#[derive(Debug, Clone)]
pub struct StateTransition {
    pub from: AgentState,
    pub to: AgentState,
    pub timestamp: i64,
    pub reason: Option<String>,
}

/// Agent state machine
pub struct AgentStateMachine {
    /// Current state
    current: AgentState,
    /// Previous state
    previous: Option<AgentState>,
    /// State entry time
    state_entered_at: Instant,
    /// Transition history
    history: Vec<StateTransition>,
    /// Max history size
    max_history: usize,
}

impl AgentStateMachine {
    /// Create a new state machine
    pub fn new() -> Self {
        Self {
            current: AgentState::Idle,
            previous: None,
            state_entered_at: Instant::now(),
            history: Vec::new(),
            max_history: 100,
        }
    }
    
    /// Get current state
    pub fn current_state(&self) -> AgentState {
        self.current
    }
    
    /// Get previous state
    pub fn previous_state(&self) -> Option<AgentState> {
        self.previous
    }
    
    /// Get time in current state
    pub fn time_in_state(&self) -> std::time::Duration {
        self.state_entered_at.elapsed()
    }
    
    /// Check if transition is valid
    pub fn can_transition_to(&self, target: AgentState) -> bool {
        use AgentState::*;
        
        match (self.current, target) {
            // From Idle
            (Idle, Opening) => true,
            (Idle, Paused) => true,
            (Idle, Error) => true,
            
            // From Opening
            (Opening, Monitoring) => true,
            (Opening, Idle) => true, // Failed to open
            (Opening, Paused) => true,
            (Opening, Error) => true,
            
            // From Monitoring
            (Monitoring, Closing) => true,
            (Monitoring, Rebalancing) => true,
            (Monitoring, Paused) => true,
            (Monitoring, Error) => true,
            
            // From Closing
            (Closing, Idle) => true,
            (Closing, Paused) => true,
            (Closing, Error) => true,
            
            // From Rebalancing
            (Rebalancing, Monitoring) => true,
            (Rebalancing, Paused) => true,
            (Rebalancing, Error) => true,
            
            // From Paused
            (Paused, Idle) => true,
            (Paused, Monitoring) => true, // Resume to monitoring if position exists
            (Paused, Closing) => true, // Force close
            (Paused, Error) => true,
            
            // From Error
            (Error, Idle) => true,
            (Error, Paused) => true,
            
            // Same state
            (a, b) if a == b => false,
            
            // All other transitions invalid
            _ => false,
        }
    }
    
    /// Transition to a new state
    pub fn transition_to(&mut self, target: AgentState) -> bool {
        self.transition_to_with_reason(target, None)
    }
    
    /// Transition to a new state with reason
    pub fn transition_to_with_reason(&mut self, target: AgentState, reason: Option<String>) -> bool {
        if !self.can_transition_to(target) {
            warn!(
                "Invalid state transition: {} -> {}",
                self.current, target
            );
            return false;
        }
        
        let transition = StateTransition {
            from: self.current,
            to: target,
            timestamp: chrono::Utc::now().timestamp_millis(),
            reason: reason.clone(),
        };
        
        info!(
            "State transition: {} -> {}{}",
            self.current,
            target,
            reason.map(|r| format!(" ({})", r)).unwrap_or_default()
        );
        
        self.previous = Some(self.current);
        self.current = target;
        self.state_entered_at = Instant::now();
        
        // Add to history
        self.history.push(transition);
        if self.history.len() > self.max_history {
            self.history.remove(0);
        }
        
        true
    }
    
    /// Get transition history
    pub fn history(&self) -> &[StateTransition] {
        &self.history
    }
    
    /// Check if in active trading state
    pub fn is_active(&self) -> bool {
        matches!(
            self.current,
            AgentState::Opening | AgentState::Monitoring | AgentState::Closing | AgentState::Rebalancing
        )
    }
    
    /// Check if paused or in error
    pub fn is_halted(&self) -> bool {
        matches!(self.current, AgentState::Paused | AgentState::Error)
    }
    
    /// Reset to idle state
    pub fn reset(&mut self) {
        self.transition_to_with_reason(AgentState::Idle, Some("Reset".to_string()));
    }
}

impl Default for AgentStateMachine {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_initial_state() {
        let sm = AgentStateMachine::new();
        assert_eq!(sm.current_state(), AgentState::Idle);
    }
    
    #[test]
    fn test_valid_transition() {
        let mut sm = AgentStateMachine::new();
        assert!(sm.transition_to(AgentState::Opening));
        assert_eq!(sm.current_state(), AgentState::Opening);
    }
    
    #[test]
    fn test_invalid_transition() {
        let mut sm = AgentStateMachine::new();
        assert!(!sm.transition_to(AgentState::Closing)); // Can't go directly to Closing
        assert_eq!(sm.current_state(), AgentState::Idle);
    }
    
    #[test]
    fn test_full_lifecycle() {
        let mut sm = AgentStateMachine::new();
        
        // Idle -> Opening -> Monitoring -> Closing -> Idle
        assert!(sm.transition_to(AgentState::Opening));
        assert!(sm.transition_to(AgentState::Monitoring));
        assert!(sm.transition_to(AgentState::Closing));
        assert!(sm.transition_to(AgentState::Idle));
        
        assert_eq!(sm.history().len(), 4);
    }
    
    #[test]
    fn test_pause_resume() {
        let mut sm = AgentStateMachine::new();
        sm.transition_to(AgentState::Opening);
        sm.transition_to(AgentState::Monitoring);
        
        // Pause
        assert!(sm.transition_to(AgentState::Paused));
        assert!(sm.is_halted());
        
        // Resume to monitoring
        assert!(sm.transition_to(AgentState::Monitoring));
        assert!(!sm.is_halted());
    }
}
