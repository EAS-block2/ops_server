use crossbeam_channel::unbounded;
use serde_yaml;
use serde::Deserialize;
use std::net::{TcpListener, TcpStream, ToSocketAddrs, Shutdown};
use std::io::{Read, Write};
use std::{str, thread, collections::HashMap, time::Duration, fs::File};
use log::{warn, error, info, debug};
use simplelog;
fn main() {
    if cfg!(debug_assertions){simplelog::CombinedLogger::init(vec![
        simplelog::TermLogger::new(simplelog::LevelFilter::Debug, simplelog::Config::default(), simplelog::TerminalMode::Mixed),
        simplelog::WriteLogger::new(simplelog::LevelFilter::Warn, simplelog::Config::default(), File::create("current.log").unwrap()),
        ]).unwrap();}
    else{simplelog::CombinedLogger::init(vec![
        simplelog::TermLogger::new(simplelog::LevelFilter::Info, simplelog::Config::default(), simplelog::TerminalMode::Mixed),
        simplelog::WriteLogger::new(simplelog::LevelFilter::Warn, simplelog::Config::default(), File::create("current.log").unwrap()),
        ]).unwrap();}
    let conf_f = File::open("config.yaml").expect("Can't File Config"); //tmp filepath
    let config: Config = serde_yaml::from_reader(conf_f).expect("Bad YAML config file!");
    config.print();
    let temphash = HashMap::new();
    let (s1, r1) = unbounded();
    let (s2, r2) = unbounded();
    let (s3, r3) = unbounded();
    let (s4, r4) = unbounded();
    let (sCon, rCon) = unbounded();
    let (mfault_s, mfault_r) = unbounded();
    let (gfault_s, gfault_r) = unbounded();
    let (sfault_s, sfault_r) = unbounded();
    let mut general_alarm = Alarm {kind: AlarmType::General, ports: config.general_ports.clone(), button_sender: s1, button_reciever: r1,faulted: temphash.clone(),
        revere_sender: s3, revere_reciever: r3, fault_send: gfault_s, fault_recv: gfault_r, activators: vec!(), active:false,did_spawn:false,did_clear:true,
        global_fault_send: mfault_s.clone()};
    let mut silent_alarm = Alarm {kind: AlarmType::Silent,ports: config.silent_ports.clone(),button_sender: s2,button_reciever: r2,faulted: temphash.clone(),
        revere_sender: s4,revere_reciever: r4,fault_send: sfault_s,fault_recv: sfault_r,activators: vec!(),active:false,did_spawn:false,did_clear:true,
        global_fault_send: mfault_s.clone()};
    drop(temphash);
    general_alarm.spawn_button();
    silent_alarm.spawn_button();
    spawn_conf(8082, sCon);
    spawn_EASrvr_fault(mfault_r, &config);
    loop {
        general_alarm.process(&config);
        silent_alarm.process(&config);
        match rCon.try_recv(){
            Ok(command) => {match command{
                1 => {general_alarm.activators.clear();
                    info!("Resetting general alarm")},
                2 => {silent_alarm.activators.clear();
                    info!("Resetting silent alarm")},
                3 => {mfault_s.send(254).unwrap();
                    info!("Resetting Failed Points List");}, //tell thread to dump faulted points
                4 => {general_alarm.activators.push("Admin-Override".to_string());
                    info!("Override: activating general alarm");},
                5 => {silent_alarm.activators.push("Admin-Override".to_string());
                    info!("Override: Activating silent alarm");}
                6 => {general_alarm.activators.clear();
                    silent_alarm.activators.clear();
                    mfault_s.send(254).unwrap();
                    info!("Resetting all alarms")}
                _ => {warn!("anomalous data in confServer channel")}
            }}
            Err(_) => ()
        }
        thread::sleep(Duration::from_secs(2));
    }
}

#[derive(Deserialize)]
struct Config{
    points: u8,
    general_ports: Vec<u32>,
    silent_ports: Vec<u32>,
    button_lookup: HashMap<String, String>,
    point_lookup: HashMap<u8, String>,
}
impl Config{
    fn print(&self){
        info!("Config file data: points={}, gp={:?}, sp={:?}", self.points, self.general_ports, self.silent_ports);
        info!("Lookup table content: {:?}", self.button_lookup);
    }
}

//Kinds of alarm
#[derive(Clone, Copy)]
enum AlarmType{
    General,
    Silent,
}
impl AlarmType{
    fn into_sendable(&mut self, who: &String) -> Vec<u8>{
        let mut tmp: String;
        match &self{
            AlarmType::General => {tmp = "General ".to_string()},
            AlarmType::Silent => {tmp = "Silent ".to_string()}
        }
        tmp.push_str(who);
        let retn = tmp.into_bytes();
        retn
    }
}
struct Alarm{
    kind: AlarmType,
    ports: Vec<u32>,
    button_sender: crossbeam_channel::Sender<String>,
    button_reciever: crossbeam_channel::Receiver<String>,
    revere_sender: crossbeam_channel::Sender<Vec<String>>,
    revere_reciever: crossbeam_channel::Receiver<Vec<String>>,
    global_fault_send: crossbeam_channel::Sender<u8>,
    fault_send: crossbeam_channel::Sender<u8>,
    fault_recv: crossbeam_channel::Receiver<u8>,
    faulted: HashMap<u8,u8>,
    activators: Vec<String>,
    active: bool,
    did_spawn: bool,
    did_clear: bool,
}
impl Alarm{
    fn process(&mut self, conf: &Config){ //does the actual work in terms of coordinating alarm conditions
        match self.button_reciever.try_recv(){ //get data from respective button thread
            Ok(who) => {
                if !self.activators.contains(&who){self.activators.push(who.clone());}}//check if alarm activator has already been recorded
            Err(e) => {
                match e {
                crossbeam_channel::TryRecvError::Empty => (), //if we get no responce it's technically an err but not a worrysome one
                crossbeam_channel::TryRecvError::Disconnected => {self.spawn_button(); //assume the buton thread crashed, try to respawn it
                }}}}
        match self.fault_recv.try_recv(){
            Ok(failed) => {
                debug!("Failed point matrix: {:?}",self.faulted);             
                self.spawn_revere(conf, failed);
                //tell easrvr about it if it failed 5 times
                let entry = self.faulted.entry(failed).or_insert(1);
                *entry += 1;
                if self.faulted[&failed] == 5{match self.global_fault_send.send(failed){
                    Ok(_)=>{debug!("told easrvr thread that point {} failed.",&failed)},
                    Err(_)=>{error!("Cannot tell easrvr about failure")}}}
            }
            Err(_)=>()
        }
        self.active = !self.activators.is_empty(); //if no activators have activated the alarm, it is off and vice versa
        if !self.active{self.did_spawn=false;
            if !self.did_clear{ //tells revere threads to tell points that all is well, then to kill themselves
                self.consume_revere_msgs();
                self.faulted.clear();
                for _ in 0..(conf.points){self.revere_sender.send(vec!("clear".to_string())).unwrap();}
                debug!("stopped reveres!");
                thread::sleep(Duration::from_secs(6));
                self.consume_revere_msgs();
                self.did_clear = true;}}
        else {
        if !self.did_spawn{for i in 0..conf.points{self.spawn_revere(conf, i);} 
            self.did_spawn = true;
            self.did_clear = false;}}
            for _ in 0..(conf.points){match self.revere_sender.send(self.activators.clone()){
                Ok(_)=>(), Err(e)=>{warn!("Revere not working due to {}", e)}}} //sends more messages than necessary, but that's preferable than too few
    }
    fn spawn_button(&self){
        for port in &self.ports{
        debug!("starting thread listening on port: {}", port);
        let sender = self.button_sender.clone();
        let mut listen_addr = "EASops:".to_string();
        listen_addr.push_str(&port.to_string());
        thread::spawn(move || {
            let tgt: std::net::SocketAddr; //real points will start at 1
            let mut addrs_iter: std::vec::IntoIter<std::net::SocketAddr>;
            match listen_addr.to_socket_addrs(){
                Ok(addr) => addrs_iter = addr,
                Err(_) => {panic!("Address resolution fault.");}}
            match addrs_iter.next(){
                Some(addr) => {tgt = addr;},
                None => {panic!("Address resolution fault.");}}
            let listener = TcpListener::bind(tgt).unwrap();
            for stream in listener.incoming() {
                match stream {
                    Ok(mut streamm) => {
                        let mut data = [0 as u8; 50];
                        match streamm.read(&mut data){
                            Ok(size) => {
                               match str::from_utf8(&data[0..size]){
                                Ok(string_out) => {
                                sender.send(string_out.to_string()).unwrap();
                                streamm.write(b"ok").unwrap();
                                }
                Err(_) => {}
                }}
                Err(_) => {}
                }}
                Err(e) => {warn!("Button thread: Connection failed with code {}", e);}
                }}
            error!("Button Listen Thread Exiting!");
        });
    }}
    //create threads to notify strobes and signs of an emergency
fn spawn_revere(&self, conf: &Config, i: u8){ //so many threads are used to ensure that a possible blocking operation only effects max. 1 point
    debug!("Starting Revere #{}", i);
    let listener = self.revere_reciever.clone();
    let errsend = self.fault_send.clone();
    let mut alarm = self.kind.clone();
    let llook = conf.button_lookup.clone();
    thread::spawn(move || {
        let mut non_ok = 0;
        let mut target = "Point".to_string();
        let tgt: std::net::SocketAddr;
            target.push_str(&i.to_string());
            if i == 0{target = "EASrvr".to_string()} //real points will start at 1
            target.push_str(":5400");
            let mut addrs_iter: std::vec::IntoIter<std::net::SocketAddr>;
            match target.to_socket_addrs(){
                Ok(addr) => addrs_iter = addr,
                Err(e) => {errsend.send(i).unwrap(); panic!("Cannot Resolve Point {} due to {}",i,e);}
            }
            match addrs_iter.next(){
                Some(addr) => {tgt = addr;},
                None => {errsend.send(i).unwrap(); panic!("Bad Address");} 
            }
        let mut msg: Vec<String> = vec!();
        let mut clear: bool = false;
        loop{
            //check for messages
            match listener.try_recv(){
                Ok(e) => {
                if e.contains(&"clear".to_string()) {clear = true;
                msg= vec!("clear".to_string());}
                    else {msg = e}}
                Err(_) => (),
            }
            for activator in msg.clone().into_iter(){
            //communicate with point
            match TcpStream::connect(tgt){
                Ok(mut stream)=>{
            stream.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
            let to_intosendable: String;
            if clear{to_intosendable = String::from("clear");}
            else {match llook.get(&activator){
                    Some(place) => {to_intosendable = place.to_string()}
                    None => {if &activator == &String::from("Admin-Override"){to_intosendable = activator}
                        else{to_intosendable = String::from("Unknown")}}
                }}
            let sendable = alarm.into_sendable(&to_intosendable);
            match stream.write(sendable.as_slice()) {Ok(_)=>(), Err(e) => {warn!("Revere Write fault! err: {}",e)}}
            let mut data = [0 as u8; 50];
            match stream.read(&mut data){
                Ok(size) => {
                    match str::from_utf8(&data[0..size]){
                        Ok(string_out) => {
                            match string_out{
                                "ok" => (),
                                &_ => {error!("point {} sent non-ok responce, that ain't good.",i);
                                        non_ok += 1;
                                        if non_ok >= 4 {panic!("Point {} sent too many erroneous responces, panicking",i)}}
                            }}
                        Err(e) => {warn!("Revere {} Client Read Error: {}",i,e);}
                    }}
                Err(e) => {warn!("Revere {} Fault when reading data: {}",i,e);}
            }
            stream.shutdown(Shutdown::Both).unwrap();
            drop(stream);
            thread::sleep(Duration::from_secs(3));}
            Err(_) => {errsend.send(i).unwrap(); panic!("Point #{}: internal fatal error", i);}
        }
    }if clear {break;}}
        debug!("Exiting Thread");
    });
} 

fn consume_revere_msgs(&self){
    let listener = self.revere_reciever.clone();
    loop{
    match listener.try_recv(){
        Ok(_) => (),
        Err(_) => break,
    }}
}
}

fn spawn_conf(port: u32, sender: crossbeam_channel::Sender<u8>){
    info!("starting Config listener on port: {}", port);
    let sender = sender.clone();
    let mut listen_addr = "EASops:".to_string();
        listen_addr.push_str(&port.to_string());
    thread::spawn(move || {
        let tgt: std::net::SocketAddr; //real points will start at 1
        let mut addrs_iter: std::vec::IntoIter<std::net::SocketAddr>;
        match listen_addr.to_socket_addrs(){
            Ok(addr) => addrs_iter = addr,
            Err(_) => {panic!("Address resolution fault.");}
        }
        match addrs_iter.next(){
            Some(addr) => {tgt = addr;},
            None => {panic!("Address resolution fault.");} 
        }
        let listener = TcpListener::bind(tgt).unwrap();
        for stream in listener.incoming() {
            match stream {
                Ok(mut streamm) => {
                    let mut data = [0 as u8; 50];
                    match streamm.read(&mut data){
                        Ok(size) => {
                           match str::from_utf8(&data[0..size]){
                            Ok(string_out) => {
                                let command: u8;
                                debug!("got config connection");
                            match string_out{
                                "gclear" => command = 1,
                                "sclear" => command = 2,
                                "fclear" => command = 3,
                                "gset"   => command = 4,
                                "sset"   => command = 5,
                                "aclear" => command = 6,
                                _ => {streamm.write(b"no").unwrap();
                                    command = 0}
                            }
                            if command != 0{sender.send(command).unwrap();
                            streamm.write(b"ok").unwrap();
                            }}
            Err(_) => {warn!("Config Server Listen: fault");}
            }}
            Err(_) => {warn!("Config Server Listen: Fault when reading data!");}
            }}
            Err(e) => {warn!("Config Server Listen: Connection failed with code {}", e);}
            }}
        error!("Config Server Listen Thread Exiting!");
    });
}

fn spawn_EASrvr_fault(reader: crossbeam_channel::Receiver<u8>, conf: &Config){
    debug!("faultsend thread started");
    let llook = conf.point_lookup.clone();
    thread::spawn(move || {
        let tgt: std::net::SocketAddr;
            let target = "EASrvr:5400".to_string();
            let mut addrs_iter: std::vec::IntoIter<std::net::SocketAddr>;
            loop {
            match target.to_socket_addrs(){
                Ok(addr) => {addrs_iter = addr;
                            break;},
                Err(_) => {error!("EASrvr is not responding!");
                            thread::sleep(Duration::from_millis(500));}
            }}
            loop{
            match addrs_iter.next(){
                Some(addr) => {tgt = addr;
                                break;},
                None => {error!("fault thread: Bad Address");
                thread::sleep(Duration::from_millis(500));}
            }}
        let mut failed: Vec<u8> = vec!();
        loop{
            //check for messages
            match reader.try_recv(){
                Ok(pt)=> {
                    if pt == 254{failed.clear();}
                    if !failed.contains(&pt){failed.push(pt);}
                }Err(_)=>{thread::sleep(Duration::from_millis(20));}
            }
            if !failed.is_empty() && !failed.contains(&254){warn!("Failed points: {:?}", failed);}
            for activator in failed.clone().into_iter(){
            //communicate with point
            match TcpStream::connect(tgt){
                Ok(mut stream)=>{
            stream.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
            let mut to_sendable = String::from("Fault ");
            if activator == 254 {to_sendable.push_str("clear");
                failed.clear();}
            else{
            match llook.get(&activator){
                    Some(place) => {to_sendable.push_str(place)}
                    None => {to_sendable.push_str("Unknown")}
                }}
                let sendable = to_sendable.into_bytes();
            match stream.write(sendable.as_slice()) {Ok(_)=>(), Err(e) => {warn!("Fault Thread: Write fault! err: {}",e)}}
            let mut data = [0 as u8; 50];
            match stream.read(&mut data){
                Ok(size) => {
                    match str::from_utf8(&data[0..size]){
                        Ok(string_out) => {
                            match string_out{
                                "ok" => (),
                                &_ => {error!("Fault thread: internal error");}
                            }}
                        Err(e) => {error!("Fault thread: Client Read Error: {}",e);}
                    }}
                Err(e) => {error!("Fault when reading data: {}", e);}
            }
            match stream.shutdown(Shutdown::Both){
                Ok(_) =>(),
                Err(_) => {error!("Config Server Error");}
            }
            drop(stream);
            thread::sleep(Duration::from_secs(3));}
            Err(_) => {error!("EASrvr: internal  error");
                    thread::sleep(Duration::from_secs(1))}
        }
    }}
    });
} 