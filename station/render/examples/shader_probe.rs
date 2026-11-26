fn main() {
 let path=std::env::args().nth(1).expect("shader pack path");
 match horizon_station_render::station_shader_pack::report(std::path::Path::new(&path)) {
  Ok(s)=>println!("{s}"),Err(e)=>{eprintln!("{e}");std::process::exit(1);}
 }
}
