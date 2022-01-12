use crate::observers::solver::ConstraintSolver;
use std::collections::HashMap;
use std::sync::RwLock;
use std::marker::PhantomData;
use std::sync::Arc;
use fugue::ir::{
    Address,
    il::ecode::Location,
    il::pcode::{Operand, PCodeOp, }
};
use fugue::bytes::{ByteCast, BE, LE};
use fuguex::concrete::hooks::{ClonableHookConcrete, HookConcrete};
use fuguex::hooks::types::{HookStepAction, HookOutcome, Error};
use fuguex::state::{
    State,
    pcode::PCodeState, StateOps};
use fuguex::machine::StepState;

    
#[derive(Debug, Clone)]
pub struct SolvingResult {
    pub target_addr: Address,
    pub value: u64,
    pub size: usize,
}

impl std::cmp::PartialEq for SolvingResult {
    fn eq(&self, eq2: &SolvingResult) -> bool {
        self.target_addr == eq2.target_addr && self.value == eq2.value
    }
}


#[derive(Debug)]
pub struct DummyPeripheral<S, E> {
    address_range_list: Vec<(Address, Address)>,
    state: PhantomData<S>,
    error: PhantomData<E>,
    event_counter: u128,    // Should be enough
    pcode_counter: u128,

    solving_started: bool,
    solving_results_cache_enable: bool,
    solving_results: Arc<RwLock<HashMap<Address, SolvingResult>>>,
    solver_default_vars: HashMap<String, u128>, // <name, values>
    solver: ConstraintSolver<LE>,
    forgive_jump: u32,

    last_mem_read_event: (Address, Address, usize, u128),  // PC, ReadAddress, size in byte, EventCounter
    last_reg_write_event: (Address, u128),

}
impl <S, E> Clone for DummyPeripheral<S, E> {
    fn clone(&self) -> Self {
        DummyPeripheral {
            address_range_list: self.address_range_list.clone(),
            state: PhantomData,
            error: PhantomData,
            pcode_counter: self.pcode_counter,
            event_counter: self.event_counter,
            solving_started: self.solving_started,
            solving_results_cache_enable: self.solving_results_cache_enable,
            solver_default_vars: self.solver_default_vars.clone(),
            last_mem_read_event: self.last_mem_read_event.clone(),     // (The address that it read data from, event_counter)
            last_reg_write_event: self.last_reg_write_event.clone(),
            solving_results: self.solving_results.clone(),
            forgive_jump: self.forgive_jump,

            solver: self.solver.clone(),
        }
    }
}

impl<S, E> DummyPeripheral<S, E>
where S: State,
    E: std::error::Error + Send + Sync + 'static{
    pub fn new() -> Self {
        Self{
            address_range_list: Vec::new(),
            state: PhantomData,
            error: PhantomData,
            pcode_counter: 0,
            event_counter: 0,
            solving_started: false,
            solving_results_cache_enable: false,
            solver_default_vars: HashMap::new(),
            last_mem_read_event: (Address::from(0u32), Address::from(0u32), 0, 0),     // (The address that it read data from, event_counter)
            last_reg_write_event: (Address::from(0u32), 0),
            solving_results: Arc::new(RwLock::new(HashMap::<Address, SolvingResult>::new())),
            forgive_jump: 0,

            solver: ConstraintSolver::new(),


        }
    }

    pub fn enable_solving_result_cache(&mut self, enable: bool){
        self.solving_results_cache_enable = enable;
    }

    pub fn add_address_range<A>(&mut self, addr_range: (A, A)) where A: Into<Address> {
        // TODO: initialize memory
        let (addr_start, addr_end) = addr_range;
        self.address_range_list.push((addr_start.into(), addr_end.into()));
    }

    pub fn add_default_reg(&mut self, name: &str, value: u128) {
        self.solver_default_vars.insert(name.to_string(), value);
    }

    pub fn get_solving_result(&self) -> Arc<RwLock<HashMap<Address, SolvingResult>>>{
        return self.solving_results.clone();
    }
}


impl<S: 'static,E> HookConcrete for DummyPeripheral<S, E>
where S: State + StateOps,
        E: std::error::Error + Send + Sync + 'static,
{
    type State = PCodeState<u8, LE>;        // TOOD: make it useful for universal endian
    type Error = E;
    type Outcome = String;


    fn hook_architectural_step(
        &mut self,
        _state: &mut Self::State,
        address: &Address,
        _operation: &StepState,
    ) -> Result<HookOutcome<HookStepAction<Self::Outcome>>, Error<Self::Error>> {
        self.event_counter += 1;
        log::trace!("PC {}", address);
        Ok(HookStepAction::Pass.into())
    }

    // 0x95e
    fn hook_operation_step(
        &mut self,
        state: &mut Self::State,
        _location: &Location,
        operation: &PCodeOp,
    ) -> Result<HookOutcome<HookStepAction<Self::Outcome>>, Error<Self::Error>>  {
        
        // Todo: check endian api
        let is_little_endian = true; //state.endian().is_little();
        // let op = pcode_istate.current().unwrap();
        match operation{
            /////////////////////
            // Control the start/end of the solving
            // Start when a value is loading from the target memory
            // Stop when a branch happens
            PCodeOp::Load {source, destination, space: _} =>{
                let source_offset =  state.get_address(source).unwrap();
                // let source_offset = if is_little_endian {
                //     state.state_ref().read_address::<LE>(source).unwrap()
                // } else {
                //     state.state_ref().read_address::<BE>(source).unwrap()
                // };
                println!("Observe load instruction src_offset:{} src: {}, dest: {}", source_offset, source, destination);
                // Check if the source address falls into the peripheral range
                for (min, max) in &self.address_range_list {
                    if *min<= source_offset && source_offset<= *max {
                        // Check if this regisiter has been solved before if have been solved, then load the previous result
                        if self.solving_results.read().unwrap().contains_key(&source_offset) && self.solving_results_cache_enable{
                            let last_result = self.solving_results.read().unwrap().get(&source_offset).unwrap().clone();
                            log::debug!("Load cached result of memory: {} value:{:#x}", source_offset, last_result.value);
                            
                            if is_little_endian {
                                let mut value_bytes = [0; 8];
                                last_result.value.into_bytes::<LE>(&mut value_bytes);
                                match destination.size() {
                                    1 => {state.set_values(source_offset, &value_bytes[0..0]).unwrap();},
                                    4 => {state.set_values(source_offset, &value_bytes[0..4]).unwrap();},
                                    8 => {state.set_values(source_offset, &value_bytes[0..8]).unwrap();},
                                    _ => { panic!("Unexpected value size for last load event");}
                                };
                            } else {
                                let mut value_bytes = [0; 8];
                                last_result.value.into_bytes::<BE>(&mut value_bytes);
                                match destination.size() {
                                    1 => {state.set_values(source_offset, &value_bytes[0..0]).unwrap();},
                                    4 => {state.set_values(source_offset, &value_bytes[0..0]).unwrap();},
                                    8 => {state.set_values(source_offset, &value_bytes[0..0]).unwrap();},
                                    _ => { panic!("Unexpected value size for last load event");}
                                };
                            }
                        }else {
                            // if not found in the previous result list, then start solving
                            self.pcode_counter = 0;
                            // record this memory read event
                            let current_pc = state.program_counter_value().unwrap();
                            // let current_pc = if is_little_endian {
                            //     state.state_ref().read_program_counter::<LE>().unwrap()
                            // } else {
                            //     state.state_ref().read_program_counter::<BE>().unwrap()
                            // };
                            self.last_mem_read_event = (current_pc, source_offset.clone(), destination.size(), self.event_counter);

                            println!("create new solver");
                            log::debug!("create new solver");
                            self.solving_started = true;            // mark the start of solving
                            self.forgive_jump = 0;
                            self.solver = ConstraintSolver::new();  // Create new solver
                            self.solver.set_default_variables(&self.solver_default_vars);   // set the default variables
                        }
                        // don't care endian for debugging message for now
                        let pc = state.program_counter_value().unwrap();
                        log::debug!("PC: {}\tLoad Source: {:?}, {}", pc, source, source_offset);
                    }
                }



            },
            PCodeOp::Store { source: _, destination, space: _} => {
                // If storing sth to that memory, then it is not a reg checking loop
                let (_pc, last_addr, _size, _last_counter) = self.last_mem_read_event;
                let dest_addr = state.get_address(destination).unwrap();
                if last_addr == dest_addr {
                    self.solving_started = false;
                }
            },
            PCodeOp::CBranch { destination, condition } =>{
                if self.solving_started {
                    let dest_addr = if let Operand::Address { value, size: _ } = destination {
                        value.offset()
                    } else {
                        // dest can be constant -> it's doing internal branching.
                        // Don't care about this case for peripheral solving
                        // don't care endian for debugging message for now
                        let pc = state.program_counter_value().unwrap();
                        log::warn!("PC: {} Destination {:?} is not an address", pc, destination);
                        0u64
                    };
                    // TODO: improve loop detection
                    let (pc, last_addr, last_size, _last_counter) = self.last_mem_read_event;
                    let last_pc = u64::from(pc);

                    // check if it is a loop
                    let mut is_loop = false;
                    if dest_addr == last_pc {
                        // If it is branching to the last memory loading instruction,
                        // then it may be a loop check for peripheral
                        log::debug!("{} {} Branched to the last memory loading instruction", dest_addr, last_pc);
                        is_loop = true;
                        self.solving_started = false;       // Mark the end of the solving
                    } else if dest_addr < last_pc{
                        // don't care endian for debugging message for now
                        let pc = state.program_counter_value().unwrap();
                        log::debug!("dest:{:?} last_read at PC {:?}, current PC {:?} Backward jumping, should be a loop", dest_addr, last_pc, pc);
                        is_loop = true;
                        self.solving_started = false;       // Mark the end of the solving
                    } else {
                        // TODO: Forware jumping inside the same function, probably staill a loop. 
                        // self.solving_started = false;       // Mark the end of the solving
                        self.forgive_jump += 1;
                        if self.forgive_jump > 1 {
                            self.solving_started = false;
                            log::debug!("{:?} {:?} Forward jumping reached forgiven value, probably not a loop", dest_addr, last_pc);
                        } else {
                            log::debug!("{:?} {:?} Forward jumping forgiven", dest_addr, last_pc);
                        }
                    }

                    // if loop detected then use the solver to get the expected value
                    if is_loop {
                        if let None = self.solver.solve(state.as_ref(), condition, 0) {
                            log::warn!("Cound not solve this value, condition {}", condition);
                        } else{
                            let solve_result = self.solver.solve(state.as_ref(), condition, 0)
                            .expect("Cound not solve this value");
                            for (k, v) in solve_result {
                                log::info!("solving result: ({}, {:?})", k, v);
                                // convert to BE byte array or LE byte array
                                let mut value_bytes = [0; 8];
                                if is_little_endian {
                                    v.unwrap().into_bytes::<LE>(&mut value_bytes);
                                } else {
                                    v.unwrap().into_bytes::<BE>(&mut value_bytes);
                                }
                                // write value to state
                                match last_size {
                                    1 => {state.set_values(k, &value_bytes[0..0]).unwrap();},
                                    4 => {state.set_values(k, &value_bytes[0..4]).unwrap();},
                                    8 => {state.set_values(k, &value_bytes[0..8]).unwrap();},
                                    _ => { panic!("Unexpected value size for last load event");}
                                };
                                // Cache the solving result
                                if self.solving_results_cache_enable {
                                    self.solving_results.write().unwrap().insert(last_addr, SolvingResult{target_addr: last_addr, value: v.unwrap(), size: last_size});
                                }
                            }
                        }
                    }
                }

            },
            PCodeOp::ICall{destination: _} => {
                self.solving_started = false; 
            },
            PCodeOp::Return { destination: _ } => {
                self.solving_started = false;
            },
            _ => {

            }
        }

        if self.solving_started {
            // If solving started, add current pcode to the solver to build the tree
            self.solver.add_pcode(operation.clone(), state.as_ref());
        }

        self.pcode_counter += 1;
        // Use state changed HookResult?
        return Ok(HookStepAction::Pass.into());

    }

}
impl<S: 'static, E> ClonableHookConcrete for DummyPeripheral<S, E>
where S: State + StateOps,
      E: std::error::Error + Send + Sync + 'static{
}