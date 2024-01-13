// SPDX-License-Identifier: AGPL-3.0-or-later

use std::{
    convert::TryFrom,
    ffi::{CStr, CString},
};

use glib_sys::g_strdup;
use libc::{c_char, c_int};

use crate::operation::{
    Operation as OperationRust,
    OperationBuilder as OperationBuilderRust,
    OperationAction as OperationActionRust
};
use crate::operation::traits::{AsOperation};
//use crate::schema::{}

/// p2panda_Operation: (free-func p2panda_operation_free)
pub struct Operation(OperationRust);

#[no_mangle]
pub extern "C" fn p2panda_operation_free(instance: *mut Operation) {
    if instance.is_null() {
        return;
    }
    unsafe {
        drop(Box::from_raw(instance));
    }
}

impl Operation {
    /// Internal method to access non-wasm instance of `Operation`.
    pub(super) fn as_inner(&self) -> &OperationRust {
        &self.0
    }
}

/*
#[no_mangle]
pub extern "C" fn p2panda_operation_get_action(instance: *mut Operation) -> OperationAction {
    let operation = unsafe {
        assert!(!instance.is_null());
        &mut *instance
    };
    return operation.0.action();
}
*/

#[no_mangle]
pub extern "C" fn p2panda_operation_has_fields(instance: *mut Operation) -> bool {
    let operation = unsafe {
        assert!(!instance.is_null());
        &mut *instance
    };
    return operation.0.has_fields();
}

#[no_mangle]
pub extern "C" fn p2panda_operation_has_previous_operations(instance: *mut Operation) -> bool {
    let operation = unsafe {
        assert!(!instance.is_null());
        &mut *instance
    };
    return operation.0.has_previous_operations();
}

#[no_mangle]
pub extern "C" fn p2panda_operation_is_create(instance: *mut Operation) -> bool {
    let operation = unsafe {
        assert!(!instance.is_null());
        &mut *instance
    };
    return operation.0.is_create();
}

#[no_mangle]
pub extern "C" fn p2panda_operation_is_update(instance: *mut Operation) -> bool {
    let operation = unsafe {
        assert!(!instance.is_null());
        &mut *instance
    };
    return operation.0.is_update();
}

#[no_mangle]
pub extern "C" fn p2panda_operation_is_delete(instance: *mut Operation) -> bool {
    let operation = unsafe {
        assert!(!instance.is_null());
        &mut *instance
    };
    return operation.0.is_delete();
}

#[no_mangle]
pub extern "C" fn p2panda_operation_get_action(instance: *mut Operation) -> OperationAction {
    let operation = unsafe {
        assert!(!instance.is_null());
        &mut *instance
    };
    OperationAction::from_rust(operation.0.action())
}

#[repr(u32)]
pub enum OperationAction {
    CREATE,
    UPDATE,
    DELETE
}

impl OperationAction {
    pub fn to_rust(&self) -> OperationActionRust {
        match self {
            OperationAction::CREATE => return OperationActionRust::Create,
            OperationAction::UPDATE => return OperationActionRust::Update,
            OperationAction::DELETE => return OperationActionRust::Delete
        }
    }

    pub fn from_rust(action: OperationActionRust) -> Self {
        match action {
            OperationActionRust::Create => OperationAction::CREATE,
            OperationActionRust::Update => OperationAction::UPDATE,
            OperationActionRust::Delete => OperationAction::DELETE
        }
    }
}

/// p2panda_OperationBuilder: (free-func p2panda_operation_builder_free)
pub struct OperationBuilder(OperationBuilderRust);

#[no_mangle]
pub extern "C" fn p2panda_operation_builder_free(instance: *mut OperationBuilder) {
    if instance.is_null() {
        return;
    }
    unsafe {
        drop(Box::from_raw(instance));
    }
}

impl OperationBuilder {
    /// Internal method to access non-wasm instance of `Operation`.
    pub(super) fn as_inner(&self) -> &OperationBuilderRust {
        &self.0
    }
}

#[no_mangle]
pub extern "C" fn p2panda_operation_builder_set_action(instance: *mut OperationBuilder, action: OperationAction) {
    let operationbuilder = unsafe {
        assert!(!instance.is_null());
        &mut *instance
    };
    operationbuilder.0 = operationbuilder.0.clone().action(action.to_rust());
}

#[no_mangle]
pub extern "C" fn p2panda_operation_builder_build(instance: *mut OperationBuilder) -> *mut Operation {
    let operationbuilder = unsafe {
        assert!(!instance.is_null());
        &mut *instance
    };
    return &mut Operation { 0: operationbuilder.0.build().unwrap() };
}

