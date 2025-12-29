use std::path::PathBuf;
use std::{env,fs};
use fret::systemstate::{stg::STGFeedbackState,stg::STGEdge,target_os::freertos::FreeRTOSSystem};
use petgraph::Direction::{Outgoing, Incoming};
use petgraph::dot::{Dot, Config};

fn main() {
    let args : Vec<String> = env::args().collect();

    let path_a = PathBuf::from(args[1].clone());
    let raw_a = fs::read(path_a).expect("Can not read dumped traces b");
    // let path_b = PathBuf::from(args[2].clone());

    let feedbackstate : STGFeedbackState<FreeRTOSSystem> = ron::from_str(&String::from_utf8_lossy(&raw_a)).expect("Can not parse HashMap");

    let mut splits = 0;
    let mut unites = 0;
    let mut g = feedbackstate.graph;
    dbg!(g.node_count());
    let mut straight = 0;
    let mut stub = 0;
    let mut done = false;
    while !done {
        done = true;
        for i in g.node_indices() {
            let li = g.neighbors_directed(i, Incoming).count();
            let lo = g.neighbors_directed(i, Outgoing).count();
            if li == 1 && lo == 1 {
                let prev = g.neighbors_directed(i, Incoming).into_iter().next().unwrap();
                let next = g.neighbors_directed(i, Outgoing).into_iter().next().unwrap();
                if prev != next {
                    g.update_edge(prev, next, STGEdge::default());
                    g.remove_node(i);
                    straight+=1;
                    done = false;
                    break;
                }
            }
        }
    }
    for i in g.node_indices() {
        let li = g.neighbors_directed(i, Incoming).count();
        if li>1 {
            unites += 1;
        }
        let lo = g.neighbors_directed(i, Outgoing).count();
        if lo>1 {
            splits += 1;
        }
        if li == 0 || lo == 0 {
            // g.remove_node(i);
            stub += 1;
        }
    }
    dbg!(splits);
    dbg!(unites);
    dbg!(straight);
    dbg!(stub);

    let newgraph = g.map(
        |_, n| n._pretty_print(),
        // |_, n| format!("{} {:?}",n.get_taskname(),n.get_input_counts().iter().min().unwrap_or(&0)),
        |_, e| e,
    );
    // let tempg = format!("{:?}",Dot::with_config(&newgraph, &[Config::EdgeNoLabel]));
    let f = format!("{:?}",Dot::with_config(&newgraph, &[Config::EdgeNoLabel]));
    let f = f.replace("\\\\n", "\n");
    let f = f.replace("\\\"", "");
    println!("{}",f);

}
