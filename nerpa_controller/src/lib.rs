/*
Copyright (c) 2021 VMware, Inc.
SPDX-License-Identifier: MIT
Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:
The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.
THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
*/

extern crate grpcio;
extern crate proto;
extern crate protobuf;

// The auto-generated crate `l2sw_ddlog` declares the `HDDlog` type.
// This serves as a reference to a running DDlog program.
// It implements `trait differential_datalog::DDlog`.
use differential_datalog::api::HDDlog;

// `differential_datalog` contains the DDlog runtime copied to each generated workspace.
use differential_datalog::DDlog; // Trait that must be implemented by DDlog program.
use differential_datalog::DDlogDynamic;
use differential_datalog::DeltaMap; // Represents a set of changes to DDlog relations.
use differential_datalog::ddval::DDValue; // Generic type wrapping all DDlog values.
use differential_datalog::program::Update;
use differential_datalog::record::{Record, IntoRecord};

use digest2ddlog::digest_to_ddlog;

use futures::{
    SinkExt,
    StreamExt,
};
use grpcio::{
    ClientDuplexReceiver,
    StreamingCallSink,
    WriteFlags,
};

use p4ext::{ActionRef, Table};

use proto::p4runtime::{
    StreamMessageRequest,
    StreamMessageResponse,
};
use proto::p4runtime_grpc::P4RuntimeClient;

use std::collections::HashMap;

// Controller serves as a handle for the Tokio tasks.
// The Tokio task can either process DDlog inputs or push outputs to the switch.
#[derive(Clone)]
pub struct Controller {
    sender: mpsc::Sender<ControllerActorMessage>,
}

impl Controller {
    pub fn new(
        switch_client: SwitchClient,
    ) -> Result<Controller, String> {
        let (sender, receiver) = mpsc::channel(8); // TODO: change channel capacity.

        let (hddlog, _) = l2sw_ddlog::run(1, false)?;
        let program = ControllerProgram::new(hddlog);

        let mut actor = ControllerActor::new(receiver, switch_client, program);
        tokio::spawn(async move { actor.run().await });

        Ok(Self{sender})
    }

    pub async fn input_to_switch(
        &self,
        input: Vec<Update<DDValue>>
    ) -> Result<(), p4ext::P4Error> {
        let (send, recv) = oneshot::channel();
        let msg = ControllerActorMessage::UpdateMessage {
            respond_to: send,
            input: input,
        };

        self.sender.send(msg).await;
        recv.await.expect("Actor task has been killed")
    }

    pub async fn stream_digests(&self) -> () {
        let (send, recv) = oneshot::channel();
        // let (send, mut rx) = mpsc::channel::<DeltaMap<DDValue>>(10);
        let msg = ControllerActorMessage::DigestMessage {
            respond_to: send,
        };

        self.sender.send(msg).await;
        // rx.recv().await.expect("Actor task has been killed")
        recv.await.expect("Actor task has been killed");
    }
}

pub struct ControllerProgram {
    hddlog: HDDlog,
}

impl ControllerProgram {
    pub fn new(hddlog: HDDlog) -> Self {
        Self{hddlog}
    }

    pub fn add_input(
        &mut self,
        updates:Vec<Update<DDValue>>
    ) -> Result<DeltaMap<DDValue>, String> {
        self.hddlog.transaction_start()?;

        match self.hddlog.apply_updates(&mut updates.into_iter()) {
            Ok(_) => {},
            Err(_) => self.hddlog.transaction_rollback()?
        };

        self.hddlog.transaction_commit_dump_changes()
    }

    pub fn dump_delta(delta: &DeltaMap<DDValue>) {
        for (rel, changes) in delta.iter() {
            println!("Changes to relation {}", l2sw_ddlog::relid2name(*rel).unwrap());
            for (val, weight) in changes.iter() {
                println!("{} {:+}", val, weight);
            }
        }
    }

    pub fn stop(&mut self) {
        self.hddlog.stop().unwrap();
    }
}

pub struct SwitchClient {
    pub client: P4RuntimeClient,
    device_id: u64,
    role_id: u64,
    target: String,
}

impl SwitchClient {
    pub fn new(
        client: P4RuntimeClient,
        p4info: String,
        opaque: String,
        cookie: String,
        action: String,
        device_id: u64,
        role_id: u64,
        target: String,
    ) -> Self {
        p4ext::set_pipeline(
            &p4info,
            &opaque,
            &cookie,
            &action,
            device_id,
            role_id,
            &target,
            &client
        );

        Self {
            client,
            device_id,
            role_id,
            target,
        }
    }

    pub fn push_outputs(&mut self, delta: &DeltaMap<DDValue>) -> Result<(), p4ext::P4Error> {
        let mut updates = Vec::new();

        let pipeline = p4ext::get_pipeline_config(self.device_id, &self.target, &self.client);
        let switch: p4ext::Switch = pipeline.get_p4info().into();

        for (_rel_id, output_map) in (*delta).clone().into_iter() {
            for (value, _weight) in output_map {
                let record = value.clone().into_record();
                
                match record {
                    Record::NamedStruct(name, recs) => {
                        // Translate the record table name to the P4 table name.
                        let mut table: Table = Table::default();
                        let mut table_name: String = "".to_string();

                        match self.get_matching_table(name.to_string(), switch.tables.clone()) {
                            Some(t) => {
                                table = t;
                                table_name = table.preamble.name;
                            },
                            None => {},
                        };

                        // Iterate through fields in the record.
                        // Map all match keys to values.
                        // If the field is the action, extract the action, name, and parameters.
                        let mut action_name: String = "".to_string();
                        let matches = &mut HashMap::<std::string::String, u16>::new();
                        let params = &mut HashMap::<std::string::String, u16>::new();
                        let mut priority: i32 = 0;
                        for (_, (fname, v)) in recs.iter().enumerate() {
                            let match_name: String = fname.to_string();

                            match match_name.as_str() {
                                "action" => {
                                    match v {
                                        Record::NamedStruct(name, arecs) => {
                                            // Find matching action name from P4 table.
                                            action_name = match self.get_matching_action_name(name.to_string(), table.actions.clone()) {
                                                Some(an) => an,
                                                None => "".to_string()
                                            };
    
                                            // Extract param values from action's records.
                                            for (_, (afname, aval)) in arecs.iter().enumerate() {
                                                params.insert(afname.to_string(), self.extract_record_value(&aval));
                                            }
                                        },
                                        _ => println!("action was incorrectly formed!")
                                    }
                                },
                                "priority" => {
                                    priority = self.extract_record_value(&v).into();
                                },
                                _ => {
                                    matches.insert(match_name, self.extract_record_value(&v));
                                }
                            }
                        }

                        // If we found a table and action, construct a P4 table entry update.
                        if !(table_name.is_empty() || action_name.is_empty()) {
                            let update = p4ext::build_table_entry_update(
                                proto::p4runtime::Update_Type::INSERT,
                                table_name.as_str(),
                                action_name.as_str(),
                                params,
                                matches,
                                priority,
                                self.device_id,
                                &self.target,
                                &self.client,
                            ).unwrap_or_else(|err| panic!("could not build table update: {}", err));
                            updates.push(update);
                        }
                    }
                    _ => {
                        println!("record was not named struct");
                    }
                }
            }
        }

        p4ext::write(updates, self.device_id, self.role_id, &self.target, &self.client)
    }

    fn extract_record_value(&mut self, r: &Record) -> u16 {
        use num_traits::cast::ToPrimitive;
        match r {
            Record::Bool(true) => 1,
            Record::Bool(false) => 0,
            Record::Int(i) => i.to_u16().unwrap_or(0),
            // TODO: Handle other types.
            _ => 1,
        }
    }

    fn get_matching_table(&mut self, record_name: String, tables: Vec<Table>) -> Option<Table> {
        for t in tables {
            let tn = &t.preamble.name;
            let tv: Vec<String> = tn.split('.').map(|s| s.to_string()).collect();
            let ts = tv.last().unwrap();

            if record_name.contains(ts) {
                return Some(t);
            }
        }

        None
    }

    fn get_matching_action_name(&mut self, record_name: String, actions: Vec<ActionRef>) -> Option<String> {
        for action_ref in actions {
            let an = action_ref.action.preamble.name;
            let av: Vec<String> = an.split('.').map(|s| s.to_string()).collect();
            let asub = av.last().unwrap();

            if record_name.contains(asub) {
                return Some(an.to_string());
            }
        }

        None
    }
}

use tokio::sync::{oneshot, mpsc};

struct ControllerActor {
    receiver: mpsc::Receiver<ControllerActorMessage>,
    switch_client: SwitchClient,
    program: ControllerProgram,
}

enum ControllerActorMessage {
    UpdateMessage {
        respond_to: oneshot::Sender<Result<(), p4ext::P4Error>>,
        input: Vec<Update<DDValue>>,
    },
    DigestMessage {
        respond_to: oneshot::Sender<DeltaMap<DDValue>>,
        // respond_to: mpsc::Sender<DeltaMap<DDValue>>,
    },
        // respond_to: mpsc::Sender<Result<StreamMessageResponse, grpcio::Error>>,
        // respond_to: mpsc::Sender<Update<DDValue>>
}

impl ControllerActor {
    fn new(
        receiver: mpsc::Receiver<ControllerActorMessage>,
        switch_client: SwitchClient,
        program: ControllerProgram,
    ) -> Self {
        ControllerActor {
            receiver,
            switch_client,
            program,
        }
    }

    async fn run(&mut self) {
        println!("top of run in controller actor");

        while let Some(msg) = self.receiver.recv().await {
            self.handle_message(msg).await;
        }
    }

    async fn handle_message(&mut self, msg: ControllerActorMessage) {        
        println!("top of handle_message");
        match msg {
            ControllerActorMessage::UpdateMessage {respond_to, input} => {
                println!("match - top of update message");
                let output = self.program.add_input(input).unwrap();
                println!("ddlog output: {:#?}", output);
                respond_to.send(self.switch_client.push_outputs(&output));
            },
            ControllerActorMessage::DigestMessage{ respond_to } => {
                println!("match - top of digest message changing");

                // Construct the configuration for a digest.
                // TODO: Delete below.
                let mau_result = p4ext::master_arbitration_update(self.switch_client.device_id, &self.switch_client.client).await;
                println!("right below mau_result");

                match mau_result {
                    Ok(_) => println!("master arbitration worked"),
                    Err(_) => println!("master arbitration error"),
                };

                // respond_to.send(mau_result).await.unwrap();
                // TODO: Delete above.

                // Configure the specific digest to send a notification to the controller per-message.
                // TODO: Move this higher up.
                let digest_id: u32 = 399590470;
                let write_response = p4ext::write_digest_config(
                    digest_id,
                    0, // max_timeout_ns
                    1, // max_list_size
                    1, // ack_timeout_ns
                    self.switch_client.device_id,
                    self.switch_client.role_id,
                    &self.switch_client.target,
                    &self.switch_client.client,
                ).await;
                println!("response to digest write: {:#?}", write_response);

                let (send, mut rx) = mpsc::channel::<Update<DDValue>>(10);

                let (sink, receiver) = self.switch_client.client.stream_channel().unwrap();

                let mut digest_actor = DigestActor::new(sink, receiver, send);
                tokio::spawn(async move { digest_actor.run().await });

                while let Some(inp) = rx.recv().await {
                    println!("input relation received: {:#?}", inp);
                    
                    let resp = self.program.add_input(vec![inp]).unwrap();
                    println!("hddlog program response: {:#?}", resp);
                    // respond_to.send(resp);
                };
            }
        }
    }
}

struct DigestActor {
    sink: StreamingCallSink<StreamMessageRequest>,
    receiver: ClientDuplexReceiver<StreamMessageResponse>,
    // respond_to: mpsc::Sender<Result<StreamMessageResponse, grpcio::Error>>
    respond_to: mpsc::Sender<Update<DDValue>>
}

impl DigestActor {
    fn new(
        sink: StreamingCallSink<StreamMessageRequest>,
        receiver: ClientDuplexReceiver<StreamMessageResponse>,
        // respond_to: mpsc::Sender<Result<StreamMessageResponse, grpcio::Error>>
        respond_to: mpsc::Sender<Update<DDValue>>
    ) -> Self {
        Self { sink, receiver, respond_to }
    }

    async fn run(&mut self) {
        // send a master arbitration update again and see if it can go through.
        use proto::p4runtime::MasterArbitrationUpdate;

        let mut update = MasterArbitrationUpdate::new();
        update.set_device_id(0);
        let mut request = StreamMessageRequest::new();
        request.set_arbitration(update);

        match self.sink.send((request, WriteFlags::default())).await {
            Ok(_) => println!("successfully sent to sink"),
            Err(e) => println!("failed to send to sink: {:#?}", e),
        };
        
        while let Some(result) = self.receiver.next().await {
            println!("digest actor received result: {:#?}", result);
            self.handle_digest(result).await;
        }
    }

    pub async fn handle_digest(&self, res: Result<StreamMessageResponse, grpcio::Error>) {
        match res {
            Err(e) => {
                println!("found stream error: {:#?}", e);
            },
            Ok(r) => {
                println!("handling digest response: {:#?}", r);
    
                let update_opt = r.update;
                if update_opt.is_none() {
                    println!("received empty update in stream response");
                }

                use proto::p4runtime::StreamMessageResponse_oneof_update::*;

                // unwrap() is safe because of none check
                match update_opt.unwrap() {
                    arbitration(_) => println!("arbitration update"),
                    packet(_) => println!("packet update"),
                    digest(d) => {
                        for data in d.get_data().iter() {
                            let update = digest_to_ddlog(d.get_digest_id(), data);

                            println!("DDlog update: {:#?}", update);
                            self.respond_to.send(update).await;
                        }
                    },
                    idle_timeout_notification(_) => println!("idle timeout update"),
                    other(_) => println!("other"),
                    error(_) => println!("error"), 
                };
            }
        }
    }
}
