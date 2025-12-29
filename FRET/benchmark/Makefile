TIME=7200

corpora/%/seed:
	mkdir -p $$(dirname $@)
	LINE=$$(grep "^$$(basename $*)" target_symbols.csv); \
	export \
	KERNEL=benchmark/build/$*.elf \
	FUZZ_MAIN=$$(echo $$LINE | cut -d, -f2) \
	FUZZ_INPUT=$$(echo $$LINE | cut -d, -f3) \
	FUZZ_INPUT_LEN=$$(echo $$LINE | cut -d, -f4) \
	BREAKPOINT=$$(echo $$LINE | cut -d, -f5) \
	SEED_DIR=benchmark/corpora/$* \
	DUMP_SEED=seed; \
	../fuzzer.sh

timedump/%$(FUZZ_RANDOM)$(SUFFIX): corpora/%/seed
	mkdir -p $$(dirname $@)
	LINE=$$(grep "^$$(basename $*)" target_symbols.csv); \
	export \
	KERNEL=benchmark/build/$*.elf \
	FUZZ_MAIN=$$(echo $$LINE | cut -d, -f2) \
	FUZZ_INPUT=$$(echo $$LINE | cut -d, -f3) \
	FUZZ_INPUT_LEN=$$(echo $$LINE | cut -d, -f4) \
	BREAKPOINT=$$(echo $$LINE | cut -d, -f5) \
	SEED_RANDOM=1 \
	TIME_DUMP=benchmark/$@ \
	CASE_DUMP=benchmark/$@; \
	../fuzzer.sh + + + + + $(TIME) + + + > $@_log
	#SEED_DIR=benchmark/corpora/$*

all_sequential: timedump/sequential/mpeg2$(FUZZ_RANDOM) timedump/sequential/dijkstra$(FUZZ_RANDOM) timedump/sequential/epic$(FUZZ_RANDOM) \
	timedump/sequential/g723_enc$(FUZZ_RANDOM) timedump/sequential/audiobeam$(FUZZ_RANDOM) \
	timedump/sequential/gsm_enc$(FUZZ_RANDOM)

all_kernel: timedump/kernel/bsort$(FUZZ_RANDOM) timedump/kernel/insertsort$(FUZZ_RANDOM) #timedump/kernel/fft$(FUZZ_RANDOM)

all_app: timedump/app/lift$(FUZZ_RANDOM)

all_system: timedump/lift$(FUZZ_RANDOM)$(SUFFIX)

all_period: timedump/waters$(FUZZ_RANDOM)$(SUFFIX)

tacle_rtos: timedump/tacle_rtos$(FUZZ_RANDOM)

graphics:
	Rscript --vanilla plot_comparison.r mnt/timedump/sequential audiobeam
	Rscript --vanilla plot_comparison.r mnt/timedump/sequential dijkstra
	Rscript --vanilla plot_comparison.r mnt/timedump/sequential epic
	Rscript --vanilla plot_comparison.r mnt/timedump/sequential g723_enc
	# Rscript --vanilla plot_comparison.r mnt/timedump/sequential gsm_enc
	# Rscript --vanilla plot_comparison.r mnt/timedump/sequential huff_dec
	Rscript --vanilla plot_comparison.r mnt/timedump/sequential mpeg2
	# Rscript --vanilla plot_comparison.r mnt/timedump/sequential rijndael_dec
	# Rscript --vanilla plot_comparison.r mnt/timedump/sequential rijndael_enc

clean:
	rm -rf corpora timedump