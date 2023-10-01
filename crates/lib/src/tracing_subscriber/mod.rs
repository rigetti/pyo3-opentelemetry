// Copyright 2023 Rigetti Computing
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! This module provides utilities for exporting spans to various backends
//! from Rust.

use rigetti_pyo3::create_init_submodule;

use self::{
    contextmanager::{
        CurrentThreadTracingConfig, GlobalTracingConfig, Tracing, TracingContextManagerError,
    },
    export_process::{
        BatchConfig, SimpleConfig, TracingInitializationError, TracingShutdownError,
        TracingStartError,
    },
};

pub(super) mod common;
mod contextmanager;
mod export_process;
pub(crate) mod layers;
pub(crate) mod subscriber;

create_init_submodule! {
    classes: [
        Tracing,
        GlobalTracingConfig,
        CurrentThreadTracingConfig,
        BatchConfig,
        SimpleConfig
    ],
    errors: [TracingContextManagerError, TracingInitializationError, TracingStartError, TracingShutdownError],
    submodules: [
        "layers": layers::init_submodule,
        "subscriber": subscriber::init_submodule
    ],
}
