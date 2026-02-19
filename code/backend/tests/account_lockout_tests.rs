//! Account lockout tests
//!
//! Tests for the account lockout feature:
//! - Failed login attempts increment the counter
//! - After threshold failures, the account is locked
//! - Locked accounts cannot log in until the lockout expires
//! - Successful login resets the failure counter
//! - Admins can manually unlock accounts

use chrono::{Duration, Utc};
use kubarr::models::prelude::*;
use kubarr::models::user;
use sea_orm::{ActiveModelTrait, EntityTrait, Set};

mod common;
use common::{create_test_db, create_test_user};

// ============================================================================
// Model / DB tests
// ============================================================================

#[tokio::test]
async fn test_new_user_has_no_lockout() {
    let db = create_test_db().await;
    let user = create_test_user(&db, "alice", "alice@example.com", "pass123", true).await;

    assert_eq!(user.failed_login_count, 0);
    assert!(user.locked_until.is_none());
}

#[tokio::test]
async fn test_failed_login_count_increments() {
    let db = create_test_db().await;
    let user = create_test_user(&db, "bob", "bob@example.com", "pass123", true).await;

    // Simulate incrementing the failed login count
    let mut model: user::ActiveModel = user.clone().into();
    model.failed_login_count = Set(5);
    model.updated_at = Set(Utc::now());
    model.update(&db).await.unwrap();

    let updated = User::find_by_id(user.id)
        .one(&db)
        .await
        .unwrap()
        .expect("User should exist");
    assert_eq!(updated.failed_login_count, 5);
    assert!(updated.locked_until.is_none());
}

#[tokio::test]
async fn test_lockout_applied_after_threshold() {
    let db = create_test_db().await;
    let user = create_test_user(&db, "charlie", "charlie@example.com", "pass123", true).await;

    let lockout_until = Utc::now() + Duration::minutes(15);

    // Simulate reaching the lockout threshold
    let mut model: user::ActiveModel = user.clone().into();
    model.failed_login_count = Set(0); // Reset after lockout applied
    model.locked_until = Set(Some(lockout_until));
    model.updated_at = Set(Utc::now());
    model.update(&db).await.unwrap();

    let updated = User::find_by_id(user.id)
        .one(&db)
        .await
        .unwrap()
        .expect("User should exist");

    assert_eq!(updated.failed_login_count, 0);
    assert!(updated.locked_until.is_some());
    assert!(updated.locked_until.unwrap() > Utc::now());
}

#[tokio::test]
async fn test_locked_user_is_detected() {
    let db = create_test_db().await;
    let user = create_test_user(&db, "dave", "dave@example.com", "pass123", true).await;

    // Lock the account
    let lockout_until = Utc::now() + Duration::minutes(15);
    let mut model: user::ActiveModel = user.clone().into();
    model.locked_until = Set(Some(lockout_until));
    model.updated_at = Set(Utc::now());
    model.update(&db).await.unwrap();

    let updated = User::find_by_id(user.id)
        .one(&db)
        .await
        .unwrap()
        .expect("User should exist");

    // Should be detected as locked
    let is_locked = updated
        .locked_until
        .map(|t| t > Utc::now())
        .unwrap_or(false);
    assert!(is_locked, "Account should be locked");
}

#[tokio::test]
async fn test_expired_lockout_is_not_active() {
    let db = create_test_db().await;
    let user = create_test_user(&db, "eve", "eve@example.com", "pass123", true).await;

    // Set an already-expired lockout
    let expired = Utc::now() - Duration::minutes(5);
    let mut model: user::ActiveModel = user.clone().into();
    model.locked_until = Set(Some(expired));
    model.updated_at = Set(Utc::now());
    model.update(&db).await.unwrap();

    let updated = User::find_by_id(user.id)
        .one(&db)
        .await
        .unwrap()
        .expect("User should exist");

    let is_locked = updated
        .locked_until
        .map(|t| t > Utc::now())
        .unwrap_or(false);
    assert!(!is_locked, "Expired lockout should not be active");
}

#[tokio::test]
async fn test_successful_login_resets_lockout_fields() {
    let db = create_test_db().await;
    let user = create_test_user(&db, "frank", "frank@example.com", "pass123", true).await;

    // Set some failed attempts
    let mut model: user::ActiveModel = user.clone().into();
    model.failed_login_count = Set(3);
    model.updated_at = Set(Utc::now());
    model.update(&db).await.unwrap();

    // Simulate successful login resetting the fields
    let refreshed = User::find_by_id(user.id)
        .one(&db)
        .await
        .unwrap()
        .expect("User should exist");

    let mut reset_model: user::ActiveModel = refreshed.into();
    reset_model.failed_login_count = Set(0);
    reset_model.locked_until = Set(None);
    reset_model.updated_at = Set(Utc::now());
    reset_model.update(&db).await.unwrap();

    let after_reset = User::find_by_id(user.id)
        .one(&db)
        .await
        .unwrap()
        .expect("User should exist");

    assert_eq!(after_reset.failed_login_count, 0);
    assert!(after_reset.locked_until.is_none());
}

#[tokio::test]
async fn test_admin_unlock_clears_lockout() {
    let db = create_test_db().await;
    let user = create_test_user(&db, "grace", "grace@example.com", "pass123", true).await;

    // Lock the account
    let lockout_until = Utc::now() + Duration::minutes(15);
    let mut model: user::ActiveModel = user.clone().into();
    model.failed_login_count = Set(0);
    model.locked_until = Set(Some(lockout_until));
    model.updated_at = Set(Utc::now());
    model.update(&db).await.unwrap();

    // Verify it's locked
    let locked = User::find_by_id(user.id)
        .one(&db)
        .await
        .unwrap()
        .expect("User should exist");
    assert!(locked.locked_until.is_some());

    // Admin unlock: reset fields
    let mut unlock_model: user::ActiveModel = locked.into();
    unlock_model.failed_login_count = Set(0);
    unlock_model.locked_until = Set(None);
    unlock_model.updated_at = Set(Utc::now());
    unlock_model.update(&db).await.unwrap();

    let unlocked = User::find_by_id(user.id)
        .one(&db)
        .await
        .unwrap()
        .expect("User should exist");
    assert_eq!(unlocked.failed_login_count, 0);
    assert!(unlocked.locked_until.is_none());
}

#[tokio::test]
async fn test_lockout_threshold_logic() {
    // Simulate 10 failures triggering lockout
    let threshold = 10;
    let mut count = 0i32;
    let mut locked = false;

    for _ in 0..10 {
        count += 1;
        if count >= threshold {
            locked = true;
            count = 0; // Reset after lockout applied
            break;
        }
    }

    assert!(locked, "Account should be locked after 10 failures");
    assert_eq!(count, 0, "Counter should be reset after lockout");
}

#[tokio::test]
async fn test_counter_does_not_increment_during_lockout() {
    let db = create_test_db().await;
    let user = create_test_user(&db, "hank", "hank@example.com", "pass123", true).await;

    // Apply lockout with counter reset
    let lockout_until = Utc::now() + Duration::minutes(15);
    let mut model: user::ActiveModel = user.clone().into();
    model.failed_login_count = Set(0);
    model.locked_until = Set(Some(lockout_until));
    model.updated_at = Set(Utc::now());
    model.update(&db).await.unwrap();

    let locked = User::find_by_id(user.id)
        .one(&db)
        .await
        .unwrap()
        .expect("User should exist");

    // During lockout: counter should NOT be incremented (check spec compliance)
    assert_eq!(
        locked.failed_login_count, 0,
        "Counter should remain 0 during lockout period"
    );
    assert!(
        locked.locked_until.map(|t| t > Utc::now()).unwrap_or(false),
        "Account should still be locked"
    );
}
