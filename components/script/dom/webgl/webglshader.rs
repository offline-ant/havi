/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

// https://www.khronos.org/registry/webgl/specs/latest/1.0/webgl.idl
use std::cell::Cell;

use canvas_traits::webgl::{
    WebGLCommand, WebGLError, WebGLResult, WebGLShaderId, webgl_channel,
};
use dom_struct::dom_struct;

use crate::dom::bindings::cell::DomRefCell;
use crate::dom::bindings::inheritance::Castable;
use crate::dom::bindings::reflector::{DomGlobal, reflect_dom_object};
use crate::dom::bindings::root::DomRoot;
use crate::dom::bindings::str::DOMString;
use crate::dom::webgl::webglobject::WebGLObject;
use crate::dom::webgl::webglrenderingcontext::{Operation, WebGLRenderingContext};
use crate::script_runtime::CanGc;

#[derive(Clone, Copy, Debug, JSTraceable, MallocSizeOf, PartialEq)]
pub(crate) enum ShaderCompilationStatus {
    NotCompiled,
    Succeeded,
    Failed,
}

#[dom_struct(associated_memory)]
pub(crate) struct WebGLShader {
    webgl_object: WebGLObject,
    #[no_trace]
    id: WebGLShaderId,
    gl_type: u32,
    source: DomRefCell<DOMString>,
    info_log: DomRefCell<DOMString>,
    marked_for_deletion: Cell<bool>,
    attached_counter: Cell<u32>,
    compilation_status: Cell<ShaderCompilationStatus>,
}

impl WebGLShader {
    fn new_inherited(context: &WebGLRenderingContext, id: WebGLShaderId, shader_type: u32) -> Self {
        Self {
            webgl_object: WebGLObject::new_inherited(context),
            id,
            gl_type: shader_type,
            source: Default::default(),
            info_log: Default::default(),
            marked_for_deletion: Cell::new(false),
            attached_counter: Cell::new(0),
            compilation_status: Cell::new(ShaderCompilationStatus::NotCompiled),
        }
    }

    pub(crate) fn maybe_new(
        context: &WebGLRenderingContext,
        shader_type: u32,
    ) -> Option<DomRoot<Self>> {
        let (sender, receiver) = webgl_channel().unwrap();
        context.send_command(WebGLCommand::CreateShader(shader_type, sender));
        receiver
            .recv()
            .unwrap()
            .map(|id| WebGLShader::new(context, id, shader_type, CanGc::note()))
    }

    pub(crate) fn new(
        context: &WebGLRenderingContext,
        id: WebGLShaderId,
        shader_type: u32,
        can_gc: CanGc,
    ) -> DomRoot<Self> {
        reflect_dom_object(
            Box::new(WebGLShader::new_inherited(context, id, shader_type)),
            &*context.global(),
            can_gc,
        )
    }
}

impl WebGLShader {
    pub(crate) fn id(&self) -> WebGLShaderId {
        self.id
    }

    pub(crate) fn gl_type(&self) -> u32 {
        self.gl_type
    }

    /// glCompileShader
    ///
    /// Sends shader source directly to the GPU driver for compilation.
    /// Makepad provides the GL context on all platforms; the driver handles
    /// GLSL validation and compilation.
    pub(crate) fn compile(&self) -> WebGLResult<()> {
        if self.marked_for_deletion.get() && !self.is_attached() {
            return Err(WebGLError::InvalidValue);
        }
        if self.compilation_status.get() != ShaderCompilationStatus::NotCompiled {
            debug!("Compiling already compiled shader {}", self.id);
        }

        let source = self.source.borrow();
        self.upcast()
            .send_command(WebGLCommand::CompileShader(self.id, source.str().to_string()));
        self.compilation_status
            .set(ShaderCompilationStatus::Succeeded);
        *self.info_log.borrow_mut() = "".into();
        Ok(())
    }

    /// Mark this shader as deleted (if it wasn't previously)
    /// and delete it as if calling glDeleteShader.
    /// Currently does not check if shader is attached
    pub(crate) fn mark_for_deletion(&self, operation_fallibility: Operation) {
        if !self.marked_for_deletion.get() {
            self.marked_for_deletion.set(true);
            self.upcast()
                .send_with_fallibility(WebGLCommand::DeleteShader(self.id), operation_fallibility);
        }
    }

    pub(crate) fn is_marked_for_deletion(&self) -> bool {
        self.marked_for_deletion.get()
    }

    pub(crate) fn is_deleted(&self) -> bool {
        self.marked_for_deletion.get() && !self.is_attached()
    }

    pub(crate) fn is_attached(&self) -> bool {
        self.attached_counter.get() > 0
    }

    pub(crate) fn increment_attached_counter(&self) {
        self.attached_counter.set(self.attached_counter.get() + 1);
    }

    pub(crate) fn decrement_attached_counter(&self) {
        assert!(self.attached_counter.get() > 0);
        self.attached_counter.set(self.attached_counter.get() - 1);
    }

    /// glGetShaderInfoLog
    pub(crate) fn info_log(&self) -> DOMString {
        self.info_log.borrow().clone()
    }

    /// Get the shader source
    pub(crate) fn source(&self) -> DOMString {
        self.source.borrow().clone()
    }

    /// glShaderSource
    pub(crate) fn set_source(&self, source: DOMString) {
        *self.source.borrow_mut() = source;
    }

    pub(crate) fn successfully_compiled(&self) -> bool {
        self.compilation_status.get() == ShaderCompilationStatus::Succeeded
    }
}

impl Drop for WebGLShader {
    fn drop(&mut self) {
        self.mark_for_deletion(Operation::Fallible);
    }
}
