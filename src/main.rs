use crossbeam_channel::unbounded;
use serde_yaml;
use serde::Deserialize;
use std::net::{TcpListener, TcpStream, ToSocketAddrs, Shutdown};
use std::io::{Read, Write};
use std::{str, thread, collections::HashMap, time::Duration};
fn main() {
    let mut test_int = 0;
    let conf_f = std::fs::File::open("/home/jake/Documents/Programming/Block2/ops_server/config.yaml").expect("e"); //tmp filepath
    let config: Config = serde_yaml::from_reader(conf_f).expect("Bad YAML config file!");
    config.print();
    let (s1, r1) = unbounded();
    let (s2, r2) = unbounded();
    let (s3, r3) = unbounded();
    let (s4, r4) = unbounded();
    let (fault_s, fault_r) = unbounded();
    let mut general_alarm = Alarm {kind: AlarmType::General, port: config.general_port.to_string(), button_sender: s1, button_reciever: r1,
        revere_sender: s3, revere_reciever: r3, fault_send: fault_s.clone(), activators: vec!(), active:false,did_spawn:false,did_clear:true};
    let mut silent_alarm = Alarm {kind: AlarmType::Silent, port: config.silent_port.to_string(), button_sender: s2, button_reciever: r2,
        revere_sender: s4, revere_reciever: r4, fault_send: fault_s.clone(), activators: vec!(), active:false,did_spawn:false,did_clear:true};
    general_alarm.spawn_button();
    silent_alarm.spawn_button();
    let mut failed: Vec<u8> = vec!(); //list of failed points
    loop {
        test_int += 1;
        general_alarm.process(&config);
        silent_alarm.process(&config);
        deal_with_faults(fault_r.clone(), &mut failed);
        println!("{}",test_int);
        if test_int == 10{test_int=0;
        general_alarm.activators.clear();
        silent_alarm.activators.clear();}
        thread::sleep(Duration::from_secs(2));
    }
}

#[derive(Deserialize)]
struct Config{
    points: u8,
    general_port: u32,
    silent_port: u32,
    loc_lookup: HashMap<String, String>,
}
impl Config{
    fn print(&self){
        println!("Config file data: points={}, gp={}, sp={}", self.points, self.general_port, self.silent_port);
        println!("Lookup table content: {:?}", self.loc_lookup);
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
    port: String,
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
        println!("starting thread listening on port: {}", self.port);
        let sender = self.button_sender.clone();
        let mut listen_addr = "192.168.1.162:".to_string();
        listen_addr.push_str(&self.port);
        thread::spawn(move || {
            let listener = TcpListener::bind(listen_addr).unwrap();
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
    }
    //create threads to notify strobes and signs of an emergency
fn spawn_revere(&self, conf: &Config){ //so many threads are used to ensure that a possible blocking operation only effects max. 1 point
    for i in 0..conf.points {
        println!("Starting Revere #{}", i);
        let listener = self.revere_reciever.clone();
        let errsend = self.fault_send.clone();
        let mut alarm = self.kind.clone();
        let llook = conf.loc_lookup.clone();
        thread::spawn(move || {
            let mut target = "Point".to_string();
            let tgt: std::net::SocketAddr;
                target.push_str(&i.to_string());
                target.push_str(":5400");
                let mut addrs_iter: std::vec::IntoIter<std::net::SocketAddr>;
                match target.to_socket_addrs(){
                    Ok(addr) => addrs_iter = addr,
                    Err(_) => {errsend.send(i).unwrap(); panic!("Cannot Resolve Point {}", i);}
                }
                match addrs_iter.next(){
                    Some(addr) => {tgt = addr;},
                    None => {errsend.send(i).unwrap(); panic!("Bad Address");} //should probably set do_ip_fallback to true //TEMPORARY
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
                        None => {to_intosendable = String::from("Nowhere")}
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
//read the fault channel and add any unresponsive points to the faults vec to be sent to admin
fn deal_with_faults(reader: crossbeam_channel::Receiver<u8>,failed: &mut Vec<u8>){
    match reader.try_recv(){
        Ok(pt)=> {
            if !failed.contains(&pt){failed.push(pt);}
        }Err(_)=>()}
    if !failed.is_empty(){println!("Failed points: {:?}", failed);}
}