use crossbeam_channel::unbounded;
use std::time::Duration;
use std::net::{TcpListener, TcpStream, ToSocketAddrs, SocketAddr, Shutdown};
use std::io::{Read, Write};
use std::{str, thread, fs};
fn main() {
    let mut testInt = 0;
    let (general_s, general_r) = unbounded();
    let (silent_s, silent_r) = unbounded();
    let mut general_alarm = Alarm {kind: AlarmType::General, port: "5432".to_string(), sender: general_s, reciever: general_r,
        activators: vec!(), active:false,did_spawn:false};
    let mut silent_alarm = Alarm {kind: AlarmType::Silent, port: "5433".to_string(), sender: silent_s, reciever: silent_r,
        activators: vec!(), active:false,did_spawn:false};
    let alarms = vec!(&general_alarm, &silent_alarm).into_iter();
    let (revere_send, revere_read) = unbounded();
    let points_f = "/home/jake/Documents/Programming/Block2/points.txt";
    let mut points_o = PointsStruct{points: 0, buttons: 0};
    points_o.load_file(points_f);
    for i in alarms{i.spawn_button();}
    loop {
        testInt += 1;
        for i in alarms{i.check_is_active(points_o.points, revere_read);}
        //if testInt == 20{testInt=0;
        //for _ in 0..(points_o.points){revere_send.send(0).unwrap()};} //kill all reveres
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
    activators: Vec<String>,
    active: bool,
    did_spawn: bool,
}
impl Alarm{
    fn check_is_active(&mut self, pts: u8, reciever: crossbeam_channel::Receiver<Vec<String>>){
        match self.reciever.try_recv(){ 
            Ok(who) => {
                if !self.activators.contains(&who){self.activators.push(who.clone());}}//check if alarm activator has already been recorded
            Err(e) => {
                match e {
                crossbeam_channel::TryRecvError::Empty => (), //if we get no responce it's technically an err
                crossbeam_channel::TryRecvError::Disconnected => {
                    panic!("FATAL: lost communications with button thread");
                }}}}
        self.active = !self.activators.is_empty();
        if !self.active{self.did_spawn=false;}
        if self.active && !self.did_spawn{self.spawn_revere(pts, reciever);
        self.did_spawn = true;}
    }
    fn spawn_button(&self){
        println!("starting thread listening on port: {}", self.port);
        let sender = self.sender.clone();
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
                                println!("Got data: {}", string_out);
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
fn spawn_revere(&self, pts: u8, reciever: crossbeam_channel::Receiver<Vec<String>>){
    for i in 0..pts {
        println!("Starting Revere #{}", i);
        let listener = reciever.clone();
        let mut do_ip_fallback = false;
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
            let mut msg: Vec<String>;
            loop{
                let mut stream = TcpStream::connect(tgt).expect("fault while connecting!");
                stream.set_read_timeout(Some(Duration::from_secs(10)));
                //check for thread close
                match listener.try_recv(){
                    Ok(e) => msg = e,
                    Err(_) => (),
                }
                println!("revere MSG is: {:?}", msg); //for testing only
                if msg.contains(&0.to_string()) {break;}
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
    }
} 
}
// Read data from a file
struct PointsStruct {
    points: u8,
    buttons: u8,}
impl PointsStruct {
    fn load_file(&mut self, file: &str) {
        let f_string = fs::read_to_string(file).expect("Something went wrong reading the file");
        let spl = f_string.split_whitespace();
        for a in spl {
            let b: u8;
            b = a.parse::<u8>().unwrap();
            if self.points == 0 {self.points = b;}
            else{self.buttons = b;}
        }
        println!("points: {:?}", self.points);
        println!("buttons: {:?}", self.buttons);
    }
}

