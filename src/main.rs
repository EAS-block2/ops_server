use std::fs;
use std::thread;
use crossbeam_channel::unbounded;
use std::time::Duration;
use std::net::{TcpListener, TcpStream};
use std::io::{Read, Write};
fn main() {
    let (s1, r1) = unbounded();
    let points_f = "/home/jake/Documents/EAS/Block2/points.txt";
    let mut points_o = PointsStruct{points:0, buttons: 0};
    points_o.load_file(points_f);
    spawn_revere(points_o.points, false, r1);
    thread::sleep(Duration::from_secs(2)); //temporary until loop is established
    for _ in 0..(points_o.points){s1.send(0).unwrap()};
    spawn_button();
    loop{
    println!("main thread running.");
    thread::sleep(Duration::from_secs(4));
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
//create threads to notify strobes and signs of an emergency
fn spawn_revere(pts: u8, do_ip_fallback: bool, reciever: crossbeam_channel::Receiver<i32>){
    let mut handles = vec![];
    for i in 0..pts {
        let listener = reciever.clone();
        let handle = thread::spawn(move || {
            let mut target = "Point".to_string();
            if do_ip_fallback {
                println!("Attempting IP lookup fallback");
            }
            else {
                target.push_str(&i.to_string());
                println!("Opening socket with {}", target);
            }
            let mut msg: String;
            loop{
                match listener.recv(){
                    Ok(e) => msg = e.to_string(),
                    Err(_) => msg = "nothing".to_string(),
                }
                println!("MSG is: {}", msg);
                if msg == 0.to_string() {break;}
            }
            println!("Exiting Thread");
        });
        handles.push(handle); //not being used rn
    }
}

// create thread for listening for sockets from buttons
fn spawn_button(){
    let bThread = thread::spawn( || {
        let listener = TcpListener::bind("192.168.1.144:5432").unwrap();
        for stream in listener.incoming() {
            match stream {
                Ok(stream) => {
                    println!("Got connection: {:?}", stream);
                    let mut data = [0 as u8; 50];
                    match stream.read(&mut data){
                        Ok(size) => {
                            println!("data: {:?}",(&data[0..size]))
                        }
                        Err(err) => {
                            println!("Fault when reading data!");
                        }
                    }
                }
                Err(e) => {
                    println!("Connection failed with code {}", e);
                }
            }
        }
        /*for stream in listener.incoming() {
            println!("Got connection with data: {:?}", stream);
            let mut stream = stream.unwrap();
            stream.write(b"Hello World\r\n").unwrap();
        }*/
        println!("Button Listen Thread Exiting!");
    });
}