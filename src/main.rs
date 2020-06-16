use std::fs;
use std::thread;
use std::time::Duration;

fn main() {
    let points_f = "/home/jake/Documents/EAS/Block2/points.txt";
    let mut points_o = PointsStruct{points:0, buttons: 0};
    points_o.load_file(points_f);
    spawn_revere(points_o.points, false);
    thread::sleep(Duration::from_secs(5)); //temporary until loop is established
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
fn spawn_revere(pts: u8, do_ip_fallback: bool){
    let mut handles = vec![];
    for i in 0..(pts+1) {
        let handle = thread::spawn(move || {
            let mut target = "Point".to_string();
            if do_ip_fallback {
                println!("Attempting IP lookup fallback");
            }
            else {
                target.push_str(&i.to_string());
                println!("Opening socket with {}", target);
            }
            thread::sleep(Duration::from_secs(1)); //for testing only
            println!("Exiting Thread");
        });
        handles.push(handle);
    }
}