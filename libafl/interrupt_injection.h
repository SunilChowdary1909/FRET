#define LIBAFL_MAX_INTERRUPT_SOURCES 6
#define LIBAFL_MAX_INTERRUPTS 128

extern volatile uint32_t libafl_interrupt_offsets[LIBAFL_MAX_INTERRUPT_SOURCES][LIBAFL_MAX_INTERRUPTS];
extern volatile uint64_t libafl_num_interrupts[LIBAFL_MAX_INTERRUPT_SOURCES];

static void libafl_timed_int_hook(void*);
void libafl_clear_int_timer( void );
void libafl_start_int_timer( void );
void libafl_send_irq(int irqn);