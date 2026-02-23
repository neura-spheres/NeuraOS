use std::collections::{HashMap, VecDeque};

/// Resolve service start order using topological sort.
pub fn resolve_order(deps: &HashMap<String, Vec<String>>) -> Result<Vec<String>, String> {
    let mut in_degree: HashMap<&str, usize> = HashMap::new();
    let mut graph: HashMap<&str, Vec<&str>> = HashMap::new();

    // Initialize
    for name in deps.keys() {
        in_degree.entry(name.as_str()).or_insert(0);
        graph.entry(name.as_str()).or_default();
    }

    // Build adjacency
    for (name, dependencies) in deps {
        for dep in dependencies {
            graph.entry(dep.as_str()).or_default().push(name.as_str());
            *in_degree.entry(name.as_str()).or_insert(0) += 1;
        }
    }

    // BFS
    let mut queue: VecDeque<&str> = VecDeque::new();
    for (name, &degree) in &in_degree {
        if degree == 0 {
            queue.push_back(name);
        }
    }

    let mut order = Vec::new();
    while let Some(node) = queue.pop_front() {
        order.push(node.to_string());
        if let Some(dependents) = graph.get(node) {
            for dep in dependents {
                if let Some(degree) = in_degree.get_mut(dep) {
                    *degree -= 1;
                    if *degree == 0 {
                        queue.push_back(dep);
                    }
                }
            }
        }
    }

    if order.len() != deps.len() {
        return Err("Circular dependency detected".to_string());
    }

    Ok(order)
}
