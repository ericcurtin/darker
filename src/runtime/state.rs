//! Container state management

use crate::storage::containers::ContainerState;

/// State machine for container lifecycle
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContainerStatus {
    Created,
    Running,
    Paused,
    Stopped,
    Dead,
}

impl ContainerStatus {
    /// Convert from ContainerState
    pub fn from_state(state: &ContainerState) -> Self {
        if state.running {
            if state.paused {
                Self::Paused
            } else {
                Self::Running
            }
        } else if state.exit_code.is_some() {
            Self::Stopped
        } else {
            Self::Created
        }
    }

    /// Get status string
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Created => "created",
            Self::Running => "running",
            Self::Paused => "paused",
            Self::Stopped => "exited",
            Self::Dead => "dead",
        }
    }
}

/// Lifecycle events for container state transitions
#[derive(Debug, Clone)]
pub enum ContainerEvent {
    Create,
    Start,
    Pause,
    Unpause,
    Stop,
    Kill,
    Die { exit_code: i32 },
    Remove,
}

impl ContainerEvent {
    /// Check if a state transition is valid
    pub fn is_valid_transition(from: ContainerStatus, event: &ContainerEvent) -> bool {
        match (from, event) {
            (ContainerStatus::Created, ContainerEvent::Start) => true,
            (ContainerStatus::Created, ContainerEvent::Remove) => true,
            (ContainerStatus::Running, ContainerEvent::Pause) => true,
            (ContainerStatus::Running, ContainerEvent::Stop) => true,
            (ContainerStatus::Running, ContainerEvent::Kill) => true,
            (ContainerStatus::Running, ContainerEvent::Die { .. }) => true,
            (ContainerStatus::Paused, ContainerEvent::Unpause) => true,
            (ContainerStatus::Paused, ContainerEvent::Stop) => true,
            (ContainerStatus::Paused, ContainerEvent::Kill) => true,
            (ContainerStatus::Stopped, ContainerEvent::Start) => true,
            (ContainerStatus::Stopped, ContainerEvent::Remove) => true,
            _ => false,
        }
    }

    /// Apply event to get new status
    pub fn apply(from: ContainerStatus, event: &ContainerEvent) -> Option<ContainerStatus> {
        if !Self::is_valid_transition(from, event) {
            return None;
        }

        match event {
            ContainerEvent::Create => Some(ContainerStatus::Created),
            ContainerEvent::Start => Some(ContainerStatus::Running),
            ContainerEvent::Pause => Some(ContainerStatus::Paused),
            ContainerEvent::Unpause => Some(ContainerStatus::Running),
            ContainerEvent::Stop | ContainerEvent::Kill | ContainerEvent::Die { .. } => {
                Some(ContainerStatus::Stopped)
            }
            ContainerEvent::Remove => Some(ContainerStatus::Dead),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    #[test]
    fn test_state_transitions() {
        // Valid: Created -> Running
        assert!(ContainerEvent::is_valid_transition(
            ContainerStatus::Created,
            &ContainerEvent::Start
        ));

        // Valid: Running -> Stopped
        assert!(ContainerEvent::is_valid_transition(
            ContainerStatus::Running,
            &ContainerEvent::Stop
        ));

        // Invalid: Created -> Pause
        assert!(!ContainerEvent::is_valid_transition(
            ContainerStatus::Created,
            &ContainerEvent::Pause
        ));
    }

    #[test]
    fn test_status_from_state() {
        let state = ContainerState {
            running: true,
            paused: false,
            pid: Some(1234),
            exit_code: None,
            started_at: Utc::now(),
            finished_at: None,
        };

        assert_eq!(ContainerStatus::from_state(&state), ContainerStatus::Running);
    }
}
