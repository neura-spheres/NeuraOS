/// Planner module for multi-step task decomposition.
/// The planner analyzes complex user requests and breaks them into tool-call steps.
pub struct Planner {
    pub max_plan_steps: u32,
}

impl Planner {
    pub fn new() -> Self {
        Self { max_plan_steps: 20 }
    }
}

impl Default for Planner {
    fn default() -> Self {
        Self::new()
    }
}
