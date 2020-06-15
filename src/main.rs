use std::env;
use std::fs;
fn main() {
    let points_f = "/home/jake/Documents/EAS/Block2/points.txt";
    let mut points_o = PointsStruct{points:0, buttons: 0};
    points_o.load_file(points_f);
}
struct PointsStruct {
    points: u8,
    buttons: u8,
}
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
