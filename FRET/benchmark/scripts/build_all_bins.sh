def_flags="--no-default-features --features std,snapshot_fast,restarting,do_hash_notify_state,trace_job_response_times,fuzz_int"
set -e
cargo build --target-dir ./bins/target_showmap ${def_flags},config_stg
cargo build --target-dir ./bins/target_random ${def_flags},feed_longest
cargo build --target-dir ./bins/target_frafl ${def_flags},config_frafl,feed_longest
cargo build --target-dir ./bins/target_afl ${def_flags},config_afl,observe_hitcounts
cargo build --target-dir ./bins/target_stg ${def_flags},config_stg
cargo build --target-dir ./bins/target_stgpath ${def_flags},feed_stg_abbhash,sched_stg_abbhash,mutate_stg
cargo build --target-dir ./bins/target_feedgeneration1 ${def_flags},feed_genetic,gensize_1
cargo build --target-dir ./bins/target_feedgeneration10 ${def_flags},feed_genetic,gensize_10
cargo build --target-dir ./bins/target_feedgeneration100 ${def_flags},feed_genetic,gensize_100
cargo build --target-dir ./bins/target_feedgeneration1000 ${def_flags},feed_genetic,gensize_1000
cargo build --target-dir ./bins/target_genetic100 ${def_flags},feed_genetic,mutate_stg,gensize_100
cargo build --target-dir ./bins/target_genetic1000 ${def_flags},feed_genetic,mutate_stg,gensize_1000

