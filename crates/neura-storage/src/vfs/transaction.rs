use super::{Vfs, VfsResult};

/// A batch of VFS operations that can be committed atomically.
/// On failure, previous operations in the batch are NOT rolled back (we use journal replay for recovery).
pub struct VfsTransaction<'a> {
    vfs: &'a Vfs,
    ops: Vec<TransactionOp>,
    committed: bool,
}

enum TransactionOp {
    Mkdir { path: String, owner: String },
    WriteFile { path: String, data: Vec<u8>, owner: String },
    Remove { path: String },
}

impl<'a> VfsTransaction<'a> {
    pub fn new(vfs: &'a Vfs) -> Self {
        Self {
            vfs,
            ops: Vec::new(),
            committed: false,
        }
    }

    pub fn mkdir(&mut self, path: impl Into<String>, owner: impl Into<String>) -> &mut Self {
        self.ops.push(TransactionOp::Mkdir {
            path: path.into(),
            owner: owner.into(),
        });
        self
    }

    pub fn write_file(&mut self, path: impl Into<String>, data: Vec<u8>, owner: impl Into<String>) -> &mut Self {
        self.ops.push(TransactionOp::WriteFile {
            path: path.into(),
            data,
            owner: owner.into(),
        });
        self
    }

    pub fn remove(&mut self, path: impl Into<String>) -> &mut Self {
        self.ops.push(TransactionOp::Remove {
            path: path.into(),
        });
        self
    }

    /// Execute all queued operations.
    pub async fn commit(mut self) -> VfsResult<()> {
        for op in &self.ops {
            match op {
                TransactionOp::Mkdir { path, owner } => {
                    self.vfs.mkdir(path, owner).await?;
                }
                TransactionOp::WriteFile { path, data, owner } => {
                    self.vfs.write_file(path, data.clone(), owner).await?;
                }
                TransactionOp::Remove { path } => {
                    self.vfs.remove(path).await?;
                }
            }
        }
        self.committed = true;
        Ok(())
    }
}
