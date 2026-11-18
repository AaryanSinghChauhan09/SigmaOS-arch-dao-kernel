//! DAO (Data Access Object) kernel for SigmaOS.
//!
//! Provides data access abstraction layer with CRUD operations, query building,
//! and transaction management. Simulates database access without actual DB.

use std::collections::HashMap;
use std::fmt;

/// Errors for DAO operations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DaoError {
    EntityNotFound(String),
    DuplicateEntity(String),
    InvalidQuery(String),
    TransactionFailed(String),
    ConnectionFailed(String),
}

impl fmt::Display for DaoError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EntityNotFound(e) => write!(f, "entity not found: {e}"),
            Self::DuplicateEntity(e) => write!(f, "duplicate entity: {e}"),
            Self::InvalidQuery(q) => write!(f, "invalid query: {q}"),
            Self::TransactionFailed(m) => write!(f, "transaction failed: {m}"),
            Self::ConnectionFailed(m) => write!(f, "connection failed: {m}"),
        }
    }
}

impl std::error::Error for DaoError {}

/// Entity identifier.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct EntityId(pub String);

impl EntityId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }
}

/// Generic entity data.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entity {
    pub id: EntityId,
    pub table: String,
    pub data: HashMap<String, String>,
    pub created_at: u64,
    pub updated_at: u64,
}

impl Entity {
    pub fn new(id: EntityId, table: impl Into<String>) -> Self {
        Self {
            id,
            table: table.into(),
            data: HashMap::new(),
            created_at: 0,
            updated_at: 0,
        }
    }

    pub fn set(&mut self, key: impl Into<String>, value: impl Into<String>) {
        self.data.insert(key.into(), value.into());
    }

    pub fn get(&self, key: &str) -> Option<&String> {
        self.data.get(key)
    }
}

/// Query builder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Query {
    pub table: String,
    pub filters: Vec<(String, String)>,
    pub limit: Option<usize>,
    pub offset: Option<usize>,
}

impl Query {
    pub fn new(table: impl Into<String>) -> Self {
        Self {
            table: table.into(),
            filters: Vec::new(),
            limit: None,
            offset: None,
        }
    }

    pub fn where_eq(mut self, field: impl Into<String>, value: impl Into<String>) -> Self {
        self.filters.push((field.into(), value.into()));
        self
    }

    pub fn limit(mut self, n: usize) -> Self {
        self.limit = Some(n);
        self
    }

    pub fn offset(mut self, n: usize) -> Self {
        self.offset = Some(n);
        self
    }

    pub fn validate(&self) -> Result<(), DaoError> {
        if self.table.trim().is_empty() {
            return Err(DaoError::InvalidQuery("table name must not be empty".into()));
        }
        for (field, _) in &self.filters {
            if field.trim().is_empty() {
                return Err(DaoError::InvalidQuery("filter field must not be empty".into()));
            }
        }
        Ok(())
    }
}

/// Transaction context.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Transaction {
    pub id: String,
    pub operations: Vec<String>,
    pub committed: bool,
}

/// DAO kernel manager.
#[derive(Debug, Clone, Default)]
pub struct DaoKernel {
    entities: HashMap<String, Entity>,
    tables: HashMap<String, Vec<EntityId>>,
    transactions: Vec<Transaction>,
    next_tx_id: u64,
}

impl DaoKernel {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn begin_transaction(&mut self) -> String {
        self.next_tx_id += 1;
        let tx_id = format!("tx-{}", self.next_tx_id);
        self.transactions.push(Transaction {
            id: tx_id.clone(),
            operations: Vec::new(),
            committed: false,
        });
        tx_id
    }

    pub fn commit_transaction(&mut self, tx_id: &str) -> Result<(), DaoError> {
        let tx = self.transactions.iter_mut()
            .find(|t| t.id == tx_id)
            .ok_or_else(|| DaoError::TransactionFailed(format!("transaction not found: {tx_id}")))?;
        tx.committed = true;
        Ok(())
    }

    pub fn rollback_transaction(&mut self, tx_id: &str) -> Result<(), DaoError> {
        let pos = self.transactions.iter()
            .position(|t| t.id == tx_id)
            .ok_or_else(|| DaoError::TransactionFailed(format!("transaction not found: {tx_id}")))?;
        self.transactions.remove(pos);
        Ok(())
    }

    pub fn insert(&mut self, entity: Entity) -> Result<EntityId, DaoError> {
        let key = format!("{}:{}", entity.table, entity.id.0);
        if self.entities.contains_key(&key) {
            return Err(DaoError::DuplicateEntity(key));
        }

        let table = entity.table.clone();
        let id = entity.id.clone();
        self.entities.insert(key, entity);

        self.tables.entry(table).or_default().push(id.clone());
        Ok(id)
    }

    pub fn update(&mut self, id: &EntityId, table: &str, key: &str, value: &str) -> Result<(), DaoError> {
        let full_key = format!("{}:{}", table, id.0);
        let entity = self.entities.get_mut(&full_key)
            .ok_or_else(|| DaoError::EntityNotFound(full_key.clone()))?;
        entity.set(key, value);
        entity.updated_at = self.next_tx_id;
        Ok(())
    }

    pub fn delete(&mut self, id: &EntityId, table: &str) -> Result<(), DaoError> {
        let full_key = format!("{}:{}", table, id.0);
        self.entities.remove(&full_key)
            .ok_or_else(|| DaoError::EntityNotFound(full_key.clone()))?;

        if let Some(ids) = self.tables.get_mut(table) {
            ids.retain(|eid| eid != id);
        }
        Ok(())
    }

    pub fn get(&self, id: &EntityId, table: &str) -> Result<&Entity, DaoError> {
        let full_key = format!("{}:{}", table, id.0);
        self.entities.get(&full_key)
            .ok_or_else(|| DaoError::EntityNotFound(full_key))
    }

    pub fn query(&self, query: Query) -> Result<Vec<&Entity>, DaoError> {
        query.validate()?;

        let table_ids = self.tables.get(&query.table)
            .ok_or_else(|| DaoError::InvalidQuery(format!("table not found: {}", query.table)))?;

        let mut results: Vec<&Entity> = Vec::new();
        for id in table_ids {
            let full_key = format!("{}:{}", query.table, id.0);
            if let Some(entity) = self.entities.get(&full_key) {
                let mut matches = true;
                for (field, value) in &query.filters {
                    if entity.get(field) != Some(&value.to_string()) {
                        matches = false;
                        break;
                    }
                }
                if matches {
                    results.push(entity);
                }
            }
        }

        if let Some(offset) = query.offset {
            results = results.into_iter().skip(offset).collect();
        }
        if let Some(limit) = query.limit {
            results.truncate(limit);
        }

        Ok(results)
    }

    pub fn list_tables(&self) -> Vec<&str> {
        self.tables.keys().map(|s| s.as_str()).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_insert_and_get_aaa() {
        // Arrange
        let mut dao = DaoKernel::new();
        let mut entity = Entity::new(EntityId::new("1"), "users");
        entity.set("name", "Alice");
        entity.set("email", "alice@example.com");

        // Act
        let id = dao.insert(entity).unwrap();
        let retrieved = dao.get(&id, "users").unwrap();

        // Assert
        assert_eq!(id, EntityId::new("1"));
        assert_eq!(retrieved.get("name"), Some(&"Alice".to_string()));
    }

    #[test]
    fn test_query_with_filter_aaa() {
        // Arrange
        let mut dao = DaoKernel::new();
        let mut e1 = Entity::new(EntityId::new("1"), "users");
        e1.set("name", "Alice");
        e1.set("age", "30");
        let mut e2 = Entity::new(EntityId::new("2"), "users");
        e2.set("name", "Bob");
        e2.set("age", "25");
        dao.insert(e1).unwrap();
        dao.insert(e2).unwrap();

        // Act
        let query = Query::new("users").where_eq("name", "Alice");
        let results = dao.query(query).unwrap();

        // Assert
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].get("name"), Some(&"Alice".to_string()));
    }

    #[test]
    fn test_transaction_aaa() {
        // Arrange
        let mut dao = DaoKernel::new();

        // Act
        let tx_id = dao.begin_transaction();
        dao.commit_transaction(&tx_id).unwrap();

        // Assert
        assert_eq!(dao.transactions.len(), 1);
        assert!(dao.transactions[0].committed);
    }
}
