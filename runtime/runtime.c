/* Reference I/O and program-lifetime heap allocation for RV32/ILP32. */
typedef int int32_t;
typedef unsigned int uint32_t;

/* Compile-time size checks: a negative array length makes compilation fail. */
typedef char rx_int32_is_four_bytes[(sizeof(int32_t) == 4) ? 1 : -1];
typedef char rx_uint32_is_four_bytes[(sizeof(uint32_t) == 4) ? 1 : -1];

extern int printf(const char *format, ...);
extern int scanf(const char *format, ...);
extern void *malloc(uint32_t size);
extern void *memcpy(void *dest, const void *src, uint32_t size);
extern void *memmove(void *dest, const void *src, uint32_t size);

/*
 * Valid course allocations have nonzero size and alignment 1 or 4. REIMU's
 * malloc result satisfies both alignments. Objects are not individually freed.
 */
void *__rx_alloc(uint32_t size, uint32_t align) {
  (void)align;
  return malloc(size);
}

int32_t get_i32(void) {
  int32_t value;
  (void)scanf("%d", &value);
  return value;
}

void print_i32(int32_t value) { (void)printf("%d", value); }

void println_i32(int32_t value) { (void)printf("%d\n", value); }

struct rx_vec {
  void *data;
  uint32_t len;
  uint32_t capacity;
};

void __rx_vec_new(void *vec) {
  struct rx_vec *v = vec;
  v->data = 0;
  v->len = 0;
  v->capacity = 0;
}

void __rx_vec_new_with_size(void *vec, uint32_t size, uint32_t element_size) {
  struct rx_vec *v = vec;
  v->data = size ? __rx_alloc(size * element_size, 4) : 0;
  v->len = size;
  v->capacity = size;
}

uint32_t __rx_vec_len(const void *vec) {
  const struct rx_vec *v = vec;
  return v->len;
}

int32_t __rx_vec_is_empty(const void *vec) {
  return __rx_vec_len(vec) == 0;
}

// internal helper with no exported symbol
static void rx_vec_grow(void *vec, uint32_t element_size) {
  struct rx_vec *v = vec;
  uint32_t capacity = v->capacity ? v->capacity * 2 : 4;
  void *data = __rx_alloc(capacity * element_size, 4);
  if (v->len)
    memcpy(data, v->data, v->len * element_size);
  v->data = data;
  v->capacity = capacity;
}

void __rx_vec_push(void *vec, const void *value, uint32_t element_size) {
  struct rx_vec *v = vec;
  if (v->len == v->capacity)
    rx_vec_grow(v, element_size);
  memcpy((char *)v->data + v->len * element_size, value, element_size);
  v->len += 1;
}

void __rx_vec_remove(void *vec, uint32_t index, void *result,
                     uint32_t element_size) {
  struct rx_vec *v = vec;
  char *element = (char *)v->data + index * element_size;
  memcpy(result, element, element_size);
  memmove(element, element + element_size,
          (v->len - index - 1) * element_size);
  v->len -= 1;
}
