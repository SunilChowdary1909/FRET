use std::collections::HashMap;
use std::path::PathBuf;
use clap::Arg;
use clap::App;
use std::{env,fs};

fn main() {
    let res = match App::new("edge_compare")
        .version("0.1.0")
        .author("Alwin Berger")
        .about("Compare Serialized Edge-Maps.")
        .arg(
            Arg::new("a")
                .short('a')
                .long("map-a")
                .required(true)
                .takes_value(true),
        )
        .arg(
            Arg::new("b")
                .short('b')
                .long("map-b")
                .required(true)
                .takes_value(true),
        )
        .try_get_matches_from(env::args())
    {
        Ok(res) => res,
        Err(err) => {
            println!(
                "Syntax: {}, --map-a <input> --map-b <input>\n{:?}",
                env::current_exe()
                    .unwrap_or_else(|_| "fuzzer".into())
                    .to_string_lossy(),
                err.info,
            );
            return;
        }
    };

    let path_a = PathBuf::from(res.value_of("a").unwrap().to_string());
    let path_b = PathBuf::from(res.value_of("b").unwrap().to_string());

    let raw_a = fs::read(path_a).expect("Can not read dumped edges a");
    let hmap_a : HashMap<(u64,u64),u64> = ron::from_str(&String::from_utf8_lossy(&raw_a)).expect("Can not parse HashMap");

    let raw_b = fs::read(path_b).expect("Can not read dumped edges b");
    let hmap_b : HashMap<(u64,u64),u64> = ron::from_str(&String::from_utf8_lossy(&raw_b)).expect("Can not parse HashMap");

    let mut a_and_b = Vec::<((u64,u64),u64)>::new();
    let mut a_and_b_differ = Vec::<((u64,u64),(u64,u64))>::new();
    let mut a_sans_b = Vec::<((u64,u64),u64)>::new();

    for i_a in hmap_a.clone() {
        match hmap_b.get(&i_a.0) {
            None => a_sans_b.push(i_a),
            Some(x) => if i_a.1 == *x {
                a_and_b.push(i_a);
            } else {
                a_and_b_differ.push((i_a.0,(i_a.1,*x)));
            }
        }
    }
    let b_sans_a : Vec<((u64,u64),u64)> = hmap_b.into_iter().filter(|x| !hmap_a.contains_key(&x.0) ).collect();

    println!("a_sans_b: {:#?}\na_and_b_differ: {:#?}\nb_sans_a: {:#?}",&a_sans_b,&a_and_b_differ,&b_sans_a);
    println!("Stats: a\\b: {} a&=b: {} a&!=b: {} b\\a: {} avb: {} jaccarde: {}",
    a_sans_b.len(),a_and_b.len(),a_and_b_differ.len(),b_sans_a.len(),
    a_and_b.len()+a_and_b_differ.len()+a_sans_b.len()+b_sans_a.len(),
    (a_and_b.len()+a_and_b_differ.len())as f64/(a_and_b.len()+a_and_b_differ.len()+a_sans_b.len()+b_sans_a.len()) as f64);
}
