[![License: MIT](https://img.shields.io/badge/License-MIT-green.svg)](https://opensource.org/licenses/MIT)

# NERPA

NERPA (Network Programming with Relational and Procedural Abstractions) seeks to enable co-development of the control plane and data plane. This document details the project's direction and organization. As of writing, the following parts are mostly unimplemented.

1. [nerpa_controlplane](nerpa_controlplane): A [DDlog program](nerpa_controlplane/nerpa.dl) serves as the control plane. Its input relations are fed from the management plane, and its output relations feed the data plane. After initial setup, this directory will include a generated DDlog crate used by the controller.
2. [nerpa_controller](nerpa_controller): An intermediate Rust [program](nerpa_controller/src/main.rs) runs the DDlog program using the generated crate.  It uses the management plane to adapt the DDlog program's input relations. It also pushes the output relations' rows into tables in the P4 switch using [P4runtime](https://p4.org/p4runtime/spec/master/P4Runtime-Spec.html).
3. nerpa_dataplane: We plan to implement the data plane in [P4](https://p4.org/p4-spec/docs/P4-16-working-spec.html) using the [table format](https://p4.org/p4-spec/docs/P4-16-working-spec.html#sec-tables). Note that this may require cross-language work, as it is unclear if this involves any Rust.

## Installation
### Build
0. Clone this repository. We will call its top-level directory  `$NERPA_DIR`. I would recommend using a fresh Ubuntu 18.04 VM for painless P4 installation.
1. Install DDlog using the provided [installation instructions](https://github.com/vmware/differential-datalog/blob/master/README.md#installation). This codebase used version [v0.36.0](https://github.com/vmware/differential-datalog/releases/tag/v0.36.0).
2. Install P4 using these [installation instructions](https://github.com/jafingerhut/p4-guide/blob/master/bin/README-install-troubleshooting.md#quick-instructions-for-successful-install-script-run). We used the install script `install-p4dev-v2.sh`. It is much more usable than the P4 README installation, and clones all necessary repositories and installs dependencies.

For better organization, create a dedicated directory for these dependencies, outside your clone of this repository. Run the installation script within this directory. Set `$NERPA_DEPS` equal to this directory's path.

3. Generate the DDlog crate using the [setup script](nerpa_controlplane/generate.sh). We do not commit this crate so that small differences in developer toolchains do not create significant hassle.
```
cd $NERPA_DIR/nerpa_controlplane
./generate.sh
``` 

4. Build the `proto` crate containing P4 Runtime structures. Install necessary dependencies (the protobuf and gRPC compilers).

```
cd $NERPA_DIR/proto
git submodule update --init
cargo install protobuf-codegen
cargo install grpcio-compiler
```

5. Build the intermediate controller program's crate.
```
cd $NERPA_DIR/nerpa_controller
cargo build
```

6. Build `ovsdb-sys`, the crate with bindings to the Open vSwitch database (ovsdb).

Include the `openvswitch/ovs` [codebase](https://github.com/openvswitch/ovs) using `git submodule`:
```
cd $NERPA_DIR/ovsdb-sys
git submodule update --init
cd ovs
```

Within the crate's `ovs` subdirectory, build and install Open vSwitch following these [instructions](https://github.com/openvswitch/ovs/blob/master/Documentation/intro/install/general.rst).

Build the crate:
```
cargo build
```

Confirm the bindings built correctly by running the tests:
```
cargo test
```

### Test
1. Set the environmental variable `NERPA_DEPS` to the directory containing Nerpa dependencies, including `behavioral-model`. In other words, the `simple_switch_grpc` binary should have the following path: `$NERPA_DEPS/behavioral-model/targets/simple_switch_grpc/simple_switch_grpc`.

2. Run the P4 Runtime library tests.
```
cd $NERPA_DIR/p4ext
cargo test
```

### Run
1. Start `simple_switch_grpc` from its build directory (`$NERPA_DEPS/behavioral-model/targets/simple_switch_grpc`).
```
./simple_switch_grpc --log-console --no-p4 -- --grpc-server-addr 0.0.0.0:50051 --cpu-port 1010
```
2. Run the intermediate controller program.
```
cd $NERPA_DIR/nerpa_controller
cargo run
```
Note that the input relations are currently hardcoded, because the  user interaction with the intermediate controller is unimplemented.  
Running this program should print:
```
Changes to relation Vlans
Vlans{.number = 11, .vlans = [1]} +1
```