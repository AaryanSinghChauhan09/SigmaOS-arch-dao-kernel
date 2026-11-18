// Integration test for SigmaOS-arch-dao-kernel
// Verifies the crate builds and links as a dependency.

use sigmaos_arch_dao_kernel::{DaoKernel, Entity, EntityId};

#[test]
fn crate_links_and_runs() {
    // Arrange
    let mut dao = DaoKernel::new();
    let mut entity = Entity::new(EntityId::new("1"), "users");
    entity.set("name", "Test");

    // Act
    let res = dao.insert(entity);

    // Assert
    assert!(res.is_ok());
    assert_eq!(dao.list_tables().len(), 1);
}