use crossbeam_channel::unbounded;
use serde_yaml;
use serde::Deserialize;
use std::net::{TcpListener, TcpStream, ToSocketAddrs, Shutdown};
use std::io::{Read, Write};
use std::{str, thread, collections::HashMap, time::Duration};
use log::{warn, error};
fn main() {
    let conf_f = std::fs::File::open("config.yaml").expect("Can't File Config"); //tmp filepath
    let config: Config = serde_yaml::from_reader(conf_f).expect("Bad YAML config file!");
    config.print();
    let (s1, r1) = unbounded();
    let (s2, r2) = unbounded();
    let (s3, r3) = unbounded();
    let (s4, r4) = unbounded();
    let (sCon, rCon) = unbounded();
    let (fault_s, fault_r) = unbounded();
    let mut general_alarm = Alarm {kind: AlarmType::General, ports: config.general_ports.clone(), button_sender: s1, button_reciever: r1,
        revere_sender: s3, revere_reciever: r3, fault_send: fault_s.clone(), activators: vec!(), active:false,did_spawn:false,did_clear:true};
    let mut silent_alarm = Alarm {kind: AlarmType::Silent, ports: config.silent_ports.clone(), button_sender: s2, button_reciever: r2,
        revere_sender: s4, revere_reciever: r4, fault_send: fault_s.clone(), activators: vec!(), active:false,did_spawn:false,did_clear:true};
    general_alarm.spawn_button();
    silent_alarm.spawn_button();
    spawn_conf(8082, sCon);
    spawn_EASrvr_fault(fault_r, &config);
    loop {
        general_alarm.process(&config);
        silent_alarm.process(&config);
        match rCon.try_recv(){
            Ok(command) => {match command{
                1 => {general_alarm.activators.clear();
                    println!("Resetting general alarm")},
                2 => {silent_alarm.activators.clear();
                    println!("Resetting silent alarm")},
                3 => {fault_s.send(254).unwrap();
                    println!("Resetting Failed Points List");}, //tell thread to dump faulted points
                4 => {general_alarm.activators.push("Admin-Override".to_string());
                    println!("Override: activating general alarm");},
                5 => {silent_alarm.activators.push("Admin-Override".to_string());
                    println!("Override: Activating silent alarm");}
                6 => {general_alarm.activators.clear();
                    silent_alarm.activators.clear();
                    fault_s.send(254).unwrap();
                    println!("Resetting all alarms")}
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
        println!("Config file data: points={}, gp={:?}, sp={:?}", self.points, self.general_ports, self.silent_ports);
        println!("Lookup table content: {:?}", self.button_lookup);
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
    fault_send: crossbeam_channel::Sender<u8>,
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
                crossbeam_channel::TryRecvError::Disconnected => {
                    self.spawn_button(); //assume the buton thread crashed, try to respawn it
                }}}}
        self.active = !self.activators.is_empty(); //if no activators have activated the alarm, it is off and vice versa
        if !self.active{self.did_spawn=false;
            if !self.did_clear{ //tells revere threads to tell points that all is well, then to kill themselves
                self.consume_revere_msgs();
                for _ in 0..(conf.points){self.revere_sender.send(vec!("clear".to_string())).unwrap();}
                println!("stopped reveres!");
                thread::sleep(Duration::from_secs(6));
                self.consume_revere_msgs();
                self.did_clear = true;}}
        else if !self.did_spawn{self.spawn_revere(conf); 
            self.did_spawn = true;
            self.did_clear = false;}
            for _ in 0..(conf.points){match self.revere_sender.send(self.activators.clone()){
                Ok(_)=>(), Err(e)=>{println!("Revere not working due to {}", e)}}} //sends more messages than necessary, but that's preferable than too few
    }
    fn spawn_button(&self){
        for port in &self.ports{
        println!("starting thread listening on port: {}", port);
        let sender = self.button_sender.clone();
        let mut listen_addr = "EASops:".to_string();
        let tgt: std::net::SocketAddr; //real points will start at 1
        listen_addr.push_str(&port.to_string());
        let mut addrs_iter: std::vec::IntoIter<std::net::SocketAddr>;
        match listen_addr.to_socket_addrs(){
            Ok(addr) => addrs_iter = addr,
            Err(_) => {panic!("Address resolution fault.");}
        }
        match addrs_iter.next(){
            Some(addr) => {tgt = addr;},
            None => {panic!("Address resolution fault.");} 
        }
        thread::spawn(move || {
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
                Err(_) => {println!("fault");}
                }}
                Err(_) => {println!("Fault when reading data!");}
                }}
                Err(e) => {println!("Connection failed with code {}", e);}
                }}
            println!("Button Listen Thread Exiting!");
        });
    }}
    //create threads to notify strobes and signs of an emergency
fn spawn_revere(&self, conf: &Config){ //so many threads are used to ensure that a possible blocking operation only effects max. 1 point
    for i in 0..conf.points {
        println!("Starting Revere #{}", i);
        let listener = self.revere_reciever.clone();
        let errsend = self.fault_send.clone();
        let mut alarm = self.kind.clone();
        let llook = conf.button_lookup.clone();
        thread::spawn(move || {
            let mut target = "Point".to_string();
            let tgt: std::net::SocketAddr;
                target.push_str(&i.to_string());
                if i == 0{target = "EASrvr".to_string()} //real points will start at 1
                target.push_str(":5400");
                let mut addrs_iter: std::vec::IntoIter<std::net::SocketAddr>;
                match target.to_socket_addrs(){
                    Ok(addr) => addrs_iter = addr,
                    Err(_) => {errsend.send(i).unwrap(); panic!("Cannot Resolve Point {}", i);}
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
                match stream.write(sendable.as_slice()) {Ok(_)=>(), Err(e) => {println!("Write fault! err: {}",e)}}
                let mut data = [0 as u8; 50];
                match stream.read(&mut data){
                    Ok(size) => {
                       match str::from_utf8(&data[0..size]){
                           Ok(string_out) => {
                               match string_out{
                                   "ok" => (),
                                   &_ => {errsend.send(i).unwrap(); panic!("Point #{}: internal fatal error", i);}
                               }}
                           Err(e) => {println!("Client Read Error: {}",e);}
                       }}
                    Err(e) => {println!("Fault when reading data: {}", e);}
                }
                stream.shutdown(Shutdown::Both).unwrap();
                drop(stream);
                thread::sleep(Duration::from_secs(3));}
                Err(_) => {errsend.send(i).unwrap(); panic!("Point #{}: internal fatal error", i);}
            }
     }if clear {break;}}
            println!("Exiting Thread");
        });
    }
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
    println!("starting Config listener on port: {}", port);
    let sender = sender.clone();
    let mut listen_addr = "EASops:".to_string();
        let tgt: std::net::SocketAddr; //real points will start at 1
        listen_addr.push_str(&port.to_string());
        let mut addrs_iter: std::vec::IntoIter<std::net::SocketAddr>;
        match listen_addr.to_socket_addrs(){
            Ok(addr) => addrs_iter = addr,
            Err(_) => {panic!("Address resolution fault.");}
        }
        match addrs_iter.next(){
            Some(addr) => {tgt = addr;},
            None => {panic!("Address resolution fault.");} 
        }
    thread::spawn(move || {
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
                                println!("got config connection");
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
            Err(_) => {println!("fault");}
            }}
            Err(_) => {println!("Fault when reading data!");}
            }}
            Err(e) => {println!("Connection failed with code {}", e);}
            }}
        println!("Button Listen Thread Exiting!");
    });
}

fn spawn_EASrvr_fault(reader0: crossbeam_channel::Receiver<u8>, conf: &Config){
    println!("faultsend thread started");
    let reader = reader0.clone();
    let llook = conf.point_lookup.clone();
    thread::spawn(move || {
        let tgt: std::net::SocketAddr;
            let target = "EASrvr:5400".to_string();
            let mut addrs_iter: std::vec::IntoIter<std::net::SocketAddr>;
            loop {
            match target.to_socket_addrs(){
                Ok(addr) => {addrs_iter = addr;
                            break;},
                Err(_) => {println!("fault thread: not responding");}
            }}
            loop{
            match addrs_iter.next(){
                Some(addr) => {tgt = addr;
                                break;},
                None => {println!("fault thread: Bad Address");}
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
            if !failed.is_empty() && !failed.contains(&254){println!("Failed points: {:?}", failed);}
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
            match stream.write(sendable.as_slice()) {Ok(_)=>(), Err(e) => {println!("Write fault! err: {}",e)}}
            let mut data = [0 as u8; 50];
            match stream.read(&mut data){
                Ok(size) => {
                    match str::from_utf8(&data[0..size]){
                        Ok(string_out) => {
                            match string_out{
                                "ok" => (),
                                &_ => {println!("fault thread: internal error");}
                            }}
                        Err(e) => {println!("Client Read Error: {}",e);}
                    }}
                Err(e) => {println!("Fault when reading data: {}", e);}
            }
            match stream.shutdown(Shutdown::Both){
                Ok(_) =>(),
                Err(_) => {println!("Config Server Error");}
            }
            drop(stream);
            thread::sleep(Duration::from_secs(3));}
            Err(_) => {println!("EASrvr: internal  error");
                    thread::sleep(Duration::from_secs(1))}
        }
    }}
    });
} 