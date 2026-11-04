/* 用 dlopen 直接验证重建出的 .so 是否符合 GDExtension 的入口约定。
 *
 * 这一步不需要 Godot：它模拟 Godot 的调用序列——
 *   dlopen -> dlsym(gdext_rust_init) -> 传入 get_proc_address 与初始化结构 ->
 *   检查返回值、结构体是否被填写 -> 调用 initialize/deinitialize 回调。
 *
 * 构建与运行：
 *   gcc -o /tmp/gdext_probe tools/gdext_probe.c -ldl
 *   /tmp/gdext_probe <path-to-libgw2_rebuild.so>
 */
#include <dlfcn.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>

typedef void *(*get_proc_addr_t)(const char *);
typedef void (*init_fn)(void *, int32_t);
typedef struct {
    int32_t level;
    void *userdata;
    init_fn initialize;
    init_fn deinitialize;
} init_t;
typedef uint8_t (*entry_t)(get_proc_addr_t, void *, init_t *);
typedef size_t (*count_t)(void);

static int proc_lookups = 0;

static void *stub_get_proc(const char *name) {
    proc_lookups++;
    (void)name;
    return NULL; /* 骨架阶段不真正取接口 */
}

int main(int argc, char **argv) {
    if (argc < 2) { printf("usage: %s <lib.so>\n", argv[0]); return 64; }

    void *h = dlopen(argv[1], RTLD_NOW);
    if (!h) { printf("FAIL dlopen: %s\n", dlerror()); return 1; }

    entry_t entry = (entry_t)dlsym(h, "gdext_rust_init");
    if (!entry) { printf("FAIL dlsym(gdext_rust_init): %s\n", dlerror()); return 1; }
    printf("ok   dlsym(gdext_rust_init) = %p\n", (void *)entry);

    count_t n_entry = (count_t)dlsym(h, "gw2_rebuild_entry_calls");
    count_t n_init = (count_t)dlsym(h, "gw2_rebuild_initialize_calls");
    count_t n_deinit = (count_t)dlsym(h, "gw2_rebuild_deinitialize_calls");

    init_t init;
    memset(&init, 0, sizeof init);
    init.level = -1;

    uint8_t rc = entry(stub_get_proc, NULL, &init);
    printf("%s entry returned %u (expect 1)\n", rc == 1 ? "ok  " : "FAIL", rc);
    printf("%s minimum_initialization_level = %d (expect 0)\n",
           init.level == 0 ? "ok  " : "FAIL", init.level);
    printf("%s initialize=%p deinitialize=%p\n",
           (init.initialize && init.deinitialize) ? "ok  " : "FAIL",
           (void *)init.initialize, (void *)init.deinitialize);

    if (init.initialize) init.initialize(init.userdata, 2 /* SCENE */);
    if (init.deinitialize) init.deinitialize(init.userdata, 2);

    int failed = (rc != 1) || (init.level != 0) || !init.initialize || !init.deinitialize;

    if (n_entry && n_init && n_deinit) {
        size_t e = n_entry(), i = n_init(), d = n_deinit();
        printf("%s counters entry=%zu init=%zu deinit=%zu\n",
               (e == 1 && i == 1 && d == 1) ? "ok  " : "FAIL", e, i, d);
        failed |= !(e == 1 && i == 1 && d == 1);
    } else {
        printf("...  counters unavailable (diagnostic symbols missing)\n");
    }

    dlclose(h);
    printf(failed ? "RESULT: FAIL\n" : "RESULT: PASS\n");
    return failed ? 2 : 0;
}
