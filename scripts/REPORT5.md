# FFI Incompatible Types Report

This file tracks types and functions that were removed from api.json because they cannot be represented in C FFI.

## Removed Functions

### NodeId Iterator Functions
These functions return Rust iterator types that cannot be represented in C:

| Function | Return Type | Reason |
|----------|-------------|--------|
| `NodeId.ancestors` | `Ancestors` | Iterator with lifetime, not FFI-safe |
| `NodeId.preceding_siblings` | `PrecedingSiblings` | Iterator with lifetime, not FFI-safe |
| `NodeId.following_siblings` | `FollowingSiblings` | Iterator with lifetime, not FFI-safe |
| `NodeId.children` | `Children` | Iterator with lifetime, not FFI-safe |
| `NodeId.reverse_children` | `ReverseChildren` | Iterator with lifetime, not FFI-safe |
| `NodeId.descendants` | `Descendants` | Iterator with lifetime, not FFI-safe |
| `NodeId.traverse` | `Traverse` | Iterator with lifetime, not FFI-safe |
| `NodeId.reverse_traverse` | `ReverseTraverse` | Iterator with lifetime, not FFI-safe |
| `NodeId.az_children` | `Children` | Iterator with lifetime, not FFI-safe |
| `NodeId.az_reverse_children` | `ReverseChildren` | Iterator with lifetime, not FFI-safe |

## Removed Types

| Type | Reason |
|------|--------|
| `Ancestors` | Iterator type with lifetime |
| `PrecedingSiblings` | Iterator type with lifetime |
| `FollowingSiblings` | Iterator type with lifetime |
| `Children` | Iterator type with lifetime |
| `ReverseChildren` | Iterator type with lifetime |
| `Descendants` | Iterator type with lifetime |
| `Traverse` | Iterator type with lifetime |
| `ReverseTraverse` | Iterator type with lifetime |
| `NodeHierarchyRef` | Reference type with lifetime |
| `NodeDataContainerRef` | Reference type with lifetime |

---

## Notes

Iterator types in Rust contain lifetimes and internal state that cannot be safely represented across FFI boundaries. For C/C++ users, alternative approaches would be:
1. Collect iterator results into a Vec and return that
2. Provide callback-based iteration APIs
3. Use index-based traversal with explicit function calls
