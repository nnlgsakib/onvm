# Legacy Code Removal - Migration Status

## ✅ Completed

1. **Removed files:**
   - `src/storage/blob_store.rs` - Legacy blob storage
   - `src/execution/program_store.rs` - Legacy program storage

2. **Updated Node struct:**
   - Replaced `blob_store` and `program_store` with `unified_store`
   - Added `execution_adapter` and `chunk_distributor`

3. **Updated ExecutionEngine:**
   - Now uses `UnifiedStore` and `ExecutionAdapter`
   - Module cache now uses `ObjectId` instead of `ProgramId`
   - Blob host functions use `ObjectId` instead of `BlobId`

## 🚧 Remaining Work

### High Priority

1. **Consensus Layer** (`src/consensus/mod.rs`)
   - Replace `BlobStore` with `UnifiedStore`
   - Replace `ProgramStore` with `ExecutionAdapter`
   - Update `reconstruct_chunk` - this was a BlobStore method, needs equivalent in UnifiedStore
   - Update all blob/program operations to use unified protocol

2. **RPC Layer** (`src/rpc/mod.rs`)
   - Update `upload_blob()` to use `unified_store.put_object()` with `ObjectType::Blob`
   - Update `deploy_program()` to use `unified_store.put_object()` with `ObjectType::WasmProgram`
   - Update `fetch_blob()` and `execute_program()` to use `unified_store`
   - Update `program_info()` to use `execution_adapter`

3. **Job Executor** (`src/execution/job_executor.rs`)
   - Replace `BlobStore` with `UnifiedStore`
   - Replace `ProgramStore` with `ExecutionAdapter`
   - Update blob loading to use `unified_store.get_object()`

4. **Sandbox** (`src/execution/sandbox.rs`)
   - Replace `BlobStore` with `UnifiedStore`
   - Replace `ProgramStore` with `ExecutionAdapter`

### Strategy

The key insight is that the legacy types (`BlobId`, `ProgramId`, `BlobMetadata`, `ProgramMetadata`) can **coexist** with the new unified protocol during migration:

```rust
// Legacy BlobId maps to ObjectId
impl BlobId {
    pub fn to_object_id(&self) -> ObjectId {
        ObjectId(self.0)
    }
}

// Legacy ProgramId maps to ObjectId
impl ProgramId {
    pub fn to_object_id(&self) -> ObjectId {
        ObjectId(self.0)
    }
}
```

This means:
- Keep legacy message types in consensus/network for now
- Convert to/from ObjectId at storage boundaries
- Gradual migration without breaking existing protocol

### Recommended Next Steps

1. Add `reconstruct_chunk` utility to `UnifiedStore` or as standalone function
2. Create adapter traits to bridge legacy types to unified storage
3. Update consensus layer to use adapters
4. Update RPC layer to use adapters
5. Update job executor and sandbox
6. Run tests
7. Remove legacy message types in phase 2

## Implementation Notes

### UnifiedStore Needs

```rust
impl UnifiedStore {
    // For consensus layer compatibility
    pub fn get_legacy_blob_metadata(&self, blob_id: &BlobId) -> Result<Option<BlobMetadata>> {
        let object_id = blob_id.to_object_id();
        let object = self.get_object_metadata(&object_id)?;
        // Convert Object to BlobMetadata for legacy code
        Ok(object.map(|o| BlobMetadata::from_object(o)))
    }
    
    pub fn get_legacy_program_metadata(&self, program_id: &ProgramId) -> Result<Option<ProgramMetadata>> {
        let object_id = program_id.to_object_id();
        let object = self.get_object_metadata(&object_id)?;
        // Convert Object to ProgramMetadata for legacy code
        Ok(object.map(|o| ProgramMetadata::from_object(o)))
    }
}
```

This allows gradual migration while keeping the system buildable and functional.
