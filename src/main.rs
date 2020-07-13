use crossbeam_channel::unbounded;
use serde_yaml;
use serde::Deserialize;
use std::net::{TcpListener, TcpStream, ToSocketAddrs, SocketAddr, Shutdown};
use std::io::{Read, Write};
use std::{str, thread, fs, collections::HashMap, time::Duration};
fn main() {
    let mut testInt = 0;
    let conf_f = std::fs::File::open("/home/jake/Documents/Programming/Block2/ops_server/config.yaml").expect("e"); //tmp filepath
    let config: Config = serde_yaml::from_reader(conf_f).expect("Bad YAML config file!");
    config.print();
    let (general_s, general_r) = unbounded();
    let (silent_s, silent_r) = unbounded();
    let general_alarm = Alarm {kind: AlarmType::General, port: 5432.to_string(), sender: general_s, reciever: general_r};
    let silent_alarm = Alarm {kind: AlarmType::Silent, port: 5433.to_string(), sender: silent_s, reciever: silent_r};
    let (revere_send, revere_read) = unbounded();
    //Weather s and r in the future, not needed currently
    spawn_button(&general_alarm);
    spawn_button(&silent_alarm);
    loop {
    testInt += 1;
    read_alarms(&general_alarm, config.points, revere_read.clone());
    read_alarms(&silent_alarm, config.points, revere_read.clone());
    if testInt == 20{testInt=0;
    for _ in 0..(config.points){revere_send.send(0).unwrap()};} //kill all reveres
    println!("main thread running.");
    thread::sleep(Duration::from_secs(2));
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
    sender: crossbeam_channel::Sender<String>,
    reciever: crossbeam_channel::Receiver<String>,
}
// Read data from a file
#[derive(Debug, Deserialize)]
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

//create threads to notify strobes and signs of an emergency
fn spawn_revere(pts: u8, do_ip_fallback: bool, alm: AlarmType, who: String, reciever: crossbeam_channel::Receiver<i32>){
    let mut handles = vec![];
    for i in 0..pts {
        println!("Starting Revere #{}", i);
        let listener = reciever.clone();
        let activator = who.clone();
        let mut alarm = alm.clone();
        let handle = thread::spawn(move || {
            let mut target = "Point".to_string();
            let tgt: std::net::SocketAddr;
            if do_ip_fallback {
                println!("Attempting IP lookup fallback");
                tgt = SocketAddr::from(([192, 168, 1, 144], 5432)); //TEMPORARY
            }
            else {
                target.push_str(&i.to_string());
                target.push_str(":5400");
                let mut addrs_iter = target.to_socket_addrs().unwrap();
                match addrs_iter.next(){
                    Some(addr) => {tgt = addr;
                    println!("target is {:?}", addr);},
                    None => {tgt = SocketAddr::from(([127, 0, 0, 1], 5400));} //should probably set do_ip_fallback to true //TEMPORARY
                }
            }
            //match stream.set_read_timeout(Some(Duration::from_secs(10))){Ok(_) =>{println!("Timeout set for 10 seconds")}, Err(_) =>()}
            let mut msg: String;
            loop{
                let mut stream = TcpStream::connect(tgt).expect("fault while connecting!");
                //check for thread close
                match listener.try_recv(){
                    Ok(e) => msg = e.to_string(),
                    Err(_) => msg = "nothing".to_string(),
                }
                //println!("MSG is: {}", msg);
                if msg == 0.to_string() {break;}
                //communicate with point
                let sendable = alarm.into_sendable(&activator);
                match stream.write(sendable.as_slice()) {Ok(inf)=>{println!("send alm data info: {}", inf)}, Err(e) => {println!("Write fault! err: {}",e)}}
                let mut data = [0 as u8; 50];
                match stream.read(&mut data){
                    Ok(size) => {
                       match str::from_utf8(&data[0..size]){
                           Ok(string_out) => {
                               println!("Got data: {}", string_out);}
                           Err(e) => {println!("Read Error: {}",e);}
                       }
                    }
                    Err(e) => {println!("Fault when reading data: {}", e);}
                }
                stream.shutdown(Shutdown::Both).unwrap();
                drop(stream);
                thread::sleep(Duration::from_secs(10));
                println!("\n");
            }
            println!("Exiting Thread");
        });
        handles.push(handle); //not being used rn
    }
}

// create 2 threads for listening for sockets from buttons
fn spawn_button(alarm_info:&Alarm){
    println!("starting thread listening on port: {}", alarm_info.port);
    let sender = alarm_info.sender.clone();
    let mut listen_addr = "192.168.1.162:".to_string();
    listen_addr.push_str(&alarm_info.port);
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
                                   println!("Got data: {}", string_out);
                                   sender.send(string_out.to_string()).unwrap();
                                   streamm.write(b"ok").unwrap();
                               }
                               Err(_) => {println!("fault");}
                           }
                        }
                        Err(_) => {println!("Fault when reading data!");}
                    }
                }
                Err(e) => {println!("Connection failed with code {}", e);}
            }
        }
        println!("Button Listen Thread Exiting!");
    });
}

fn read_alarms(alarm:&Alarm, points_num: u8, reciever: crossbeam_channel::Receiver<i32>){
    match alarm.reciever.try_recv(){ 
        Ok(who) => {
            spawn_revere(points_num, false, alarm.kind, who, reciever);} //TODO: add actual fallback support
        Err(e) => {
            drop(reciever);
            match e {
            crossbeam_channel::TryRecvError::Empty => (), //if we get no responce it's technically an err
            crossbeam_channel::TryRecvError::Disconnected => {
                panic!("FATAL: lost communications with button thread");
            }}}
    }
}