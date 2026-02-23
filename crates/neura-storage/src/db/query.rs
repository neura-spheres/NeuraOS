

/// Simple query builder for common operations.
pub struct QueryBuilder {
    table: String,
    conditions: Vec<String>,
    params: Vec<String>,
    order_by: Option<String>,
    limit: Option<u32>,
}

impl QueryBuilder {
    pub fn select(table: &str) -> Self {
        Self {
            table: table.to_string(),
            conditions: Vec::new(),
            params: Vec::new(),
            order_by: None,
            limit: None,
        }
    }

    pub fn where_eq(mut self, column: &str, value: &str) -> Self {
        self.conditions.push(format!("{} = ?", column));
        self.params.push(value.to_string());
        self
    }

    pub fn order_by(mut self, column: &str, desc: bool) -> Self {
        let dir = if desc { "DESC" } else { "ASC" };
        self.order_by = Some(format!("{} {}", column, dir));
        self
    }

    pub fn limit(mut self, n: u32) -> Self {
        self.limit = Some(n);
        self
    }

    pub fn build(&self) -> String {
        let mut sql = format!("SELECT * FROM {}", self.table);
        if !self.conditions.is_empty() {
            sql.push_str(" WHERE ");
            sql.push_str(&self.conditions.join(" AND "));
        }
        if let Some(ref order) = self.order_by {
            sql.push_str(&format!(" ORDER BY {}", order));
        }
        if let Some(limit) = self.limit {
            sql.push_str(&format!(" LIMIT {}", limit));
        }
        sql
    }

    pub fn params(&self) -> &[String] {
        &self.params
    }
}
