#include <errno.h>
#include <fcntl.h>
#include <limits.h>
#include <sched.h>
#include <signal.h>
#include <stdbool.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/mount.h>
#include <sys/prctl.h>
#include <sys/socket.h>
#include <sys/stat.h>
#include <sys/syscall.h>
#include <sys/types.h>
#include <sys/un.h>
#include <sys/wait.h>
#include <unistd.h>

#ifndef PR_SET_CHILD_SUBREAPER
#define PR_SET_CHILD_SUBREAPER 36
#endif

#ifndef CLONE_NEWNS
#define CLONE_NEWNS 0x00020000
#endif

#ifndef SYS_unshare
#ifdef __NR_unshare
#define SYS_unshare __NR_unshare
#endif
#endif

#ifndef SYS_pivot_root
#ifdef __NR_pivot_root
#define SYS_pivot_root __NR_pivot_root
#endif
#endif

#ifndef PATH_MAX
#define PATH_MAX 4096
#endif

#define MAX_ARGS 128
#define MAX_STRING 65535U

static volatile sig_atomic_t pending_signal;
static volatile sig_atomic_t stop_server;

static void handle_signal(int sig) {
    if (sig == SIGTERM || sig == SIGINT || sig == SIGHUP || sig == SIGQUIT) {
        stop_server = 1;
        pending_signal = sig;
    }
}

static void usage(const char *argv0) {
    fprintf(stderr,
            "usage:\n"
            "  %s --server --socket PATH --pid-file PATH [--pivot-root PATH|--native-root PATH]\n"
            "  %s --client --socket PATH --pid-file PATH [--name NAME] -- COMMAND [ARG...]\n"
            "  %s --launch [--log-file PATH] [--chroot PATH|--pivot-root PATH] -- COMMAND [ARG...]\n"
            "  %s --pid-file PATH [--name NAME] -- COMMAND [ARG...]\n",
            argv0, argv0, argv0, argv0);
}

static int write_all(int fd, const void *data, size_t size) {
    const char *ptr = data;
    while (size > 0) {
        ssize_t done = write(fd, ptr, size);
        if (done < 0) {
            if (errno == EINTR) {
                continue;
            }
            return -1;
        }
        ptr += done;
        size -= (size_t)done;
    }
    return 0;
}

static int read_all(int fd, void *data, size_t size) {
    char *ptr = data;
    while (size > 0) {
        ssize_t done = read(fd, ptr, size);
        if (done < 0) {
            if (errno == EINTR) {
                continue;
            }
            return -1;
        }
        if (done == 0) {
            errno = ECONNRESET;
            return -1;
        }
        ptr += done;
        size -= (size_t)done;
    }
    return 0;
}

static int write_u32(int fd, uint32_t value) {
    return write_all(fd, &value, sizeof(value));
}

static int read_u32(int fd, uint32_t *value) {
    return read_all(fd, value, sizeof(*value));
}

static int write_string(int fd, const char *value) {
    size_t len = strlen(value);
    if (len > MAX_STRING) {
        errno = E2BIG;
        return -1;
    }
    if (write_u32(fd, (uint32_t)len) < 0) {
        return -1;
    }
    return write_all(fd, value, len);
}

static char *read_string(int fd) {
    uint32_t len = 0;
    if (read_u32(fd, &len) < 0 || len > MAX_STRING) {
        errno = EPROTO;
        return NULL;
    }
    char *value = calloc((size_t)len + 1, 1);
    if (value == NULL) {
        return NULL;
    }
    if (read_all(fd, value, len) < 0) {
        free(value);
        return NULL;
    }
    return value;
}

static int write_pid_file_for(const char *path, pid_t pid) {
    char buffer[64];
    int len = snprintf(buffer, sizeof(buffer), "%ld\n", (long)pid);
    int fd = open(path, O_WRONLY | O_CREAT | O_TRUNC | O_CLOEXEC, 0644);
    if (fd < 0) {
        return -1;
    }
    ssize_t written = write(fd, buffer, (size_t)len);
    int saved_errno = errno;
    close(fd);
    if (written != len) {
        errno = saved_errno;
        return -1;
    }
    return 0;
}

static int install_handler(int sig) {
    struct sigaction action;
    memset(&action, 0, sizeof(action));
    action.sa_handler = handle_signal;
    sigemptyset(&action.sa_mask);
    action.sa_flags = 0;
    if (sig == SIGCHLD) {
        action.sa_flags |= SA_NOCLDSTOP;
    }
    return sigaction(sig, &action, NULL);
}

static int install_handlers(void) {
    return install_handler(SIGTERM) < 0 || install_handler(SIGINT) < 0 || install_handler(SIGHUP) < 0 ||
           install_handler(SIGQUIT) < 0 || install_handler(SIGCHLD) < 0;
}

static void reset_child_signals(void) {
    signal(SIGTERM, SIG_DFL);
    signal(SIGINT, SIG_DFL);
    signal(SIGHUP, SIG_DFL);
    signal(SIGQUIT, SIG_DFL);
    signal(SIGCHLD, SIG_DFL);
    signal(SIGPIPE, SIG_DFL);
}

static int exit_code_from_status(int status, bool have_status) {
    if (!have_status) {
        return 0;
    }
    if (WIFEXITED(status)) {
        return WEXITSTATUS(status);
    }
    if (WIFSIGNALED(status)) {
        return 128 + WTERMSIG(status);
    }
    return 1;
}

static void forward_signal(pid_t child, int sig) {
    if (child <= 0) {
        return;
    }
    kill(-child, sig);
    kill(child, sig);
}

static void reap_children(const char *name) {
    for (;;) {
        int status = 0;
        pid_t reaped = waitpid(-1, &status, WNOHANG);
        if (reaped > 0) {
            fprintf(stderr, "%s: reaped pid=%ld status=%d\n", name, (long)reaped, exit_code_from_status(status, true));
            continue;
        }
        if (reaped < 0 && errno != ECHILD && errno != EINTR) {
            perror("waitpid");
        }
        return;
    }
}

static int connect_socket(const char *socket_path) {
    int fd = socket(AF_UNIX, SOCK_STREAM, 0);
    if (fd < 0) {
        return -1;
    }

    struct sockaddr_un addr;
    memset(&addr, 0, sizeof(addr));
    addr.sun_family = AF_UNIX;
    if (strlen(socket_path) >= sizeof(addr.sun_path)) {
        close(fd);
        errno = ENAMETOOLONG;
        return -1;
    }
    strncpy(addr.sun_path, socket_path, sizeof(addr.sun_path) - 1);
    if (connect(fd, (struct sockaddr *)&addr, sizeof(addr)) < 0) {
        int saved_errno = errno;
        close(fd);
        errno = saved_errno;
        return -1;
    }
    return fd;
}

static int send_client_request(const char *socket_path, const char *pid_file, const char *name, int argc, char **argv) {
    int fd = connect_socket(socket_path);
    if (fd < 0) {
        perror("connect supervisor socket");
        return 1;
    }

    if (write_string(fd, name) < 0 || write_string(fd, pid_file) < 0 || write_u32(fd, (uint32_t)argc) < 0) {
        perror("write supervisor request");
        close(fd);
        return 1;
    }
    for (int i = 0; i < argc; i++) {
        if (write_string(fd, argv[i]) < 0) {
            perror("write supervisor argv");
            close(fd);
            return 1;
        }
    }

    char response[256];
    ssize_t size = read(fd, response, sizeof(response) - 1);
    int saved_errno = errno;
    close(fd);
    if (size < 0) {
        errno = saved_errno;
        perror("read supervisor response");
        return 1;
    }
    response[size] = '\0';
    fputs(response, stderr);
    return strncmp(response, "OK ", 3) == 0 ? 0 : 1;
}

static void free_argv(char **argv, uint32_t argc) {
    if (argv == NULL) {
        return;
    }
    for (uint32_t i = 0; i < argc; i++) {
        free(argv[i]);
    }
    free(argv);
}

static void handle_client(int fd) {
    char *name = read_string(fd);
    char *pid_file = read_string(fd);
    uint32_t argc = 0;
    if (name == NULL || pid_file == NULL || read_u32(fd, &argc) < 0 || argc == 0 || argc > MAX_ARGS) {
        dprintf(fd, "ERR malformed request\n");
        free(name);
        free(pid_file);
        return;
    }

    char **child_argv = calloc((size_t)argc + 1, sizeof(char *));
    if (child_argv == NULL) {
        dprintf(fd, "ERR out of memory\n");
        free(name);
        free(pid_file);
        return;
    }
    for (uint32_t i = 0; i < argc; i++) {
        child_argv[i] = read_string(fd);
        if (child_argv[i] == NULL) {
            dprintf(fd, "ERR malformed argv\n");
            free_argv(child_argv, argc);
            free(name);
            free(pid_file);
            return;
        }
    }

    pid_t child = fork();
    if (child < 0) {
        dprintf(fd, "ERR fork failed: %s\n", strerror(errno));
        free_argv(child_argv, argc);
        free(name);
        free(pid_file);
        return;
    }

    if (child == 0) {
        reset_child_signals();
        setpgid(0, 0);
        execvp(child_argv[0], child_argv);
        perror(child_argv[0]);
        _exit(errno == ENOENT ? 127 : 126);
    }

    setpgid(child, child);
    if (write_pid_file_for(pid_file, child) < 0) {
        int saved_errno = errno;
        kill(child, SIGTERM);
        dprintf(fd, "ERR write pid file failed: %s\n", strerror(saved_errno));
    } else {
        dprintf(fd, "OK %s pid=%ld\n", name, (long)child);
        fprintf(stderr, "achost-supervise: started %s pid=%ld\n", name, (long)child);
    }

    free_argv(child_argv, argc);
    free(name);
    free(pid_file);
}

static int path_join(char *buffer, size_t size, const char *root, const char *path) {
    int len = snprintf(buffer, size, "%s/%s", root, path[0] == '/' ? path + 1 : path);
    if (len < 0 || (size_t)len >= size) {
        errno = ENAMETOOLONG;
        return -1;
    }
    return 0;
}

static int ensure_directory(const char *path, mode_t mode) {
    struct stat st;
    if (mkdir(path, mode) < 0) {
        if (errno == EEXIST && stat(path, &st) == 0 && S_ISDIR(st.st_mode)) {
            chmod(path, mode);
            return 0;
        }
        fprintf(stderr, "mkdir %s: %s\n", path, strerror(errno));
        return -1;
    }
    chmod(path, mode);
    return 0;
}

static int make_private_mount_namespace(void) {
#if !defined(SYS_unshare)
    fprintf(stderr, "mount namespace unavailable on this platform\n");
    return 1;
#else
    if (syscall(SYS_unshare, CLONE_NEWNS) < 0) {
        perror("unshare(CLONE_NEWNS)");
        return 1;
    }
    if (mount(NULL, "/", NULL, MS_REC | MS_PRIVATE, NULL) < 0) {
        perror("mount(MS_PRIVATE)");
        return 1;
    }
    return 0;
#endif
}

static int pivot_into_root(const char *new_root) {
#if !defined(SYS_pivot_root)
    (void)new_root;
    fprintf(stderr, "pivot-root unavailable on this platform\n");
    return 1;
#else
    if (chdir(new_root) < 0) {
        perror("chdir pivot root");
        return 1;
    }
    rmdir(".achost-old-root");
    if (mkdir(".achost-old-root", 0700) < 0) {
        perror("mkdir old root");
        return 1;
    }
    if (syscall(SYS_pivot_root, ".", ".achost-old-root") < 0) {
        perror("pivot_root");
        rmdir(".achost-old-root");
        return 1;
    }
    if (chdir("/") < 0) {
        perror("chdir /");
        return 1;
    }
    if (umount2("/.achost-old-root", MNT_DETACH) < 0) {
        perror("umount old root");
        return 1;
    }
    rmdir("/.achost-old-root");
    return 0;
#endif
}

static int pivot_to_root(const char *new_root) {
    if (make_private_mount_namespace() != 0) {
        return 1;
    }
    return pivot_into_root(new_root);
}

static int bind_native_path(const char *native_root, const char *source, bool required) {
    struct stat st;
    char destination[PATH_MAX];
    if (stat(source, &st) < 0) {
        if (required) {
            fprintf(stderr, "required native path missing: %s\n", source);
            return 1;
        }
        return 0;
    }
    if (!S_ISDIR(st.st_mode)) {
        return 0;
    }
    if (path_join(destination, sizeof(destination), native_root, source) < 0) {
        perror("native path join");
        return required ? 1 : 0;
    }
    if (ensure_directory(destination, 0755) < 0) {
        return required ? 1 : 0;
    }
    if (mount(source, destination, NULL, MS_BIND | MS_REC, NULL) < 0) {
        fprintf(stderr, "bind %s to %s: %s\n", source, destination, strerror(errno));
        return required ? 1 : 0;
    }
    return 0;
}

static int mount_native_cgroup_controller(const char *cgroup_root, const char *controller) {
    char destination[PATH_MAX];

    if (path_join(destination, sizeof(destination), cgroup_root, controller) < 0) {
        perror("native cgroup path join");
        return 1;
    }
    if (ensure_directory(destination, 0755) < 0) {
        return 1;
    }
    if (mount("none", destination, "cgroup", 0, controller) < 0) {
        fprintf(stderr, "warning: unable to mount %s cgroup: %s\n", controller, strerror(errno));
        return 0;
    }
    return 0;
}

static int setup_native_cgroups(const char *native_root) {
    static const char *controllers[] = {"devices", "pids", "cpu", "cpuacct", "cpuset", "blkio", "freezer", "memory"};
    char cgroup_root[PATH_MAX];

    if (path_join(cgroup_root, sizeof(cgroup_root), native_root, "/sys/fs/cgroup") < 0) {
        perror("native cgroup root path join");
        return 1;
    }
    if (ensure_directory(cgroup_root, 0755) < 0) {
        return 1;
    }
    if (mount("tmpfs", cgroup_root, "tmpfs", 0, "mode=755,size=1m") < 0) {
        fprintf(stderr, "mount native /sys/fs/cgroup tmpfs: %s\n", strerror(errno));
        return 1;
    }
    for (size_t i = 0; i < sizeof(controllers) / sizeof(controllers[0]); i++) {
        if (mount_native_cgroup_controller(cgroup_root, controllers[i]) != 0) {
            return 1;
        }
    }
    return 0;
}

static int setup_native_run(const char *native_root) {
    char run_path[PATH_MAX];
    char var_path[PATH_MAX];
    char var_run_path[PATH_MAX];
    char tmp_path[PATH_MAX];
    struct stat st;

    if (path_join(run_path, sizeof(run_path), native_root, "/run") < 0 ||
        path_join(var_path, sizeof(var_path), native_root, "/var") < 0 ||
        path_join(var_run_path, sizeof(var_run_path), native_root, "/var/run") < 0 ||
        path_join(tmp_path, sizeof(tmp_path), native_root, "/tmp") < 0) {
        perror("native run path join");
        return 1;
    }
    if (ensure_directory(run_path, 0755) < 0 || ensure_directory(var_path, 0755) < 0 || ensure_directory(tmp_path, 01777) < 0) {
        return 1;
    }
    if (mount("tmpfs", run_path, "tmpfs", 0, "mode=755,size=64m") < 0) {
        fprintf(stderr, "mount private /run: %s\n", strerror(errno));
        return 1;
    }
    if (lstat(var_run_path, &st) == 0) {
        if (S_ISLNK(st.st_mode)) {
            unlink(var_run_path);
        } else if (S_ISDIR(st.st_mode)) {
            if (mount(run_path, var_run_path, NULL, MS_BIND | MS_REC, NULL) < 0) {
                fprintf(stderr, "bind private /run to /var/run: %s\n", strerror(errno));
                return 1;
            }
            goto tmp_mount;
        } else {
            unlink(var_run_path);
        }
    }
    if (symlink("/run", var_run_path) < 0 && errno != EEXIST) {
        fprintf(stderr, "symlink /var/run: %s\n", strerror(errno));
        return 1;
    }

tmp_mount:
    if (mount("tmpfs", tmp_path, "tmpfs", 0, "mode=1777,size=64m") < 0) {
        fprintf(stderr, "warning: unable to mount private /tmp: %s\n", strerror(errno));
    }
    return 0;
}

static int setup_native_root(const char *native_root) {
#if !defined(SYS_unshare) || !defined(SYS_pivot_root)
    (void)native_root;
    fprintf(stderr, "native root unavailable on this platform\n");
    return 1;
#else
    static const struct {
        const char *path;
        bool required;
    } mounts[] = {
        {"/data", true},
        {"/dev", true},
        {"/proc", true},
        {"/sys", true},
        {"/system", true},
        {"/apex", true},
        {"/vendor", false},
        {"/product", false},
        {"/odm", false},
        {"/mnt", false},
        {"/storage", false},
        {"/metadata", false},
        {"/linkerconfig", false},
        {"/acct", false},
        {"/config", false},
        {"/debug_ramdisk", false},
        {"/second_stage_resources", false},
        {"/sdcard", false},
    };

    if (ensure_directory(native_root, 0755) < 0) {
        return 1;
    }
    if (make_private_mount_namespace() != 0) {
        return 1;
    }
    if (mount(native_root, native_root, NULL, MS_BIND, NULL) < 0) {
        fprintf(stderr, "bind native root %s: %s\n", native_root, strerror(errno));
        return 1;
    }
    for (size_t i = 0; i < sizeof(mounts) / sizeof(mounts[0]); i++) {
        if (bind_native_path(native_root, mounts[i].path, mounts[i].required) != 0) {
            return 1;
        }
    }
    if (setup_native_cgroups(native_root) != 0) {
        return 1;
    }
    if (setup_native_run(native_root) != 0) {
        return 1;
    }
    fprintf(stderr, "achost-supervise: native root=%s private-run=ready private-cgroup=ready\n", native_root);
    return pivot_into_root(native_root);
#endif
}

static int run_launch(const char *chroot_path, const char *pivot_root_path, const char *log_file, int command_index, char **argv) {
    if (log_file != NULL) {
        int fd = open(log_file, O_WRONLY | O_CREAT | O_APPEND | O_CLOEXEC, 0644);
        if (fd < 0) {
            perror("open log file");
            return 1;
        }
        if (dup2(fd, STDOUT_FILENO) < 0 || dup2(fd, STDERR_FILENO) < 0) {
            perror("dup2 log file");
            close(fd);
            return 1;
        }
        if (fd != STDOUT_FILENO && fd != STDERR_FILENO) {
            close(fd);
        }
    }

    if (pivot_root_path != NULL) {
        if (pivot_to_root(pivot_root_path) != 0) {
            return 1;
        }
    } else if (chroot_path != NULL) {
        if (chroot(chroot_path) < 0) {
            perror("chroot");
            return 1;
        }
        if (chdir("/") < 0) {
            perror("chdir");
            return 1;
        }
    }

    execvp(argv[command_index], &argv[command_index]);
    int saved_errno = errno;
    perror(argv[command_index]);
    return saved_errno == ENOENT ? 127 : 126;
}

static int create_server_socket(const char *socket_path) {
    int fd = socket(AF_UNIX, SOCK_STREAM, 0);
    if (fd < 0) {
        return -1;
    }

    struct sockaddr_un addr;
    memset(&addr, 0, sizeof(addr));
    addr.sun_family = AF_UNIX;
    if (strlen(socket_path) >= sizeof(addr.sun_path)) {
        close(fd);
        errno = ENAMETOOLONG;
        return -1;
    }
    strncpy(addr.sun_path, socket_path, sizeof(addr.sun_path) - 1);
    unlink(socket_path);
    if (bind(fd, (struct sockaddr *)&addr, sizeof(addr)) < 0) {
        int saved_errno = errno;
        close(fd);
        errno = saved_errno;
        return -1;
    }
    chmod(socket_path, 0600);
    if (listen(fd, 8) < 0) {
        int saved_errno = errno;
        close(fd);
        unlink(socket_path);
        errno = saved_errno;
        return -1;
    }
    return fd;
}

static int run_server(const char *socket_path, const char *pid_file, const char *pivot_root_path, const char *native_root_path) {
    if (install_handlers()) {
        perror("sigaction");
        return 1;
    }
    signal(SIGPIPE, SIG_IGN);
    if (prctl(PR_SET_CHILD_SUBREAPER, 1, 0, 0, 0) < 0) {
        perror("prctl(PR_SET_CHILD_SUBREAPER)");
        return 1;
    }
    if (pivot_root_path != NULL && pivot_to_root(pivot_root_path) != 0) {
        return 1;
    }
    if (native_root_path != NULL && setup_native_root(native_root_path) != 0) {
        return 1;
    }

    int server_fd = create_server_socket(socket_path);
    if (server_fd < 0) {
        perror("create supervisor socket");
        return 1;
    }
    if (write_pid_file_for(pid_file, getpid()) < 0) {
        perror("write supervisor pid file");
        close(server_fd);
        unlink(socket_path);
        return 1;
    }

    fprintf(stderr, "achost-supervise: server pid=%ld socket=%s\n", (long)getpid(), socket_path);
    while (!stop_server) {
        reap_children("achost-supervise");
        int client_fd = accept(server_fd, NULL, NULL);
        if (client_fd < 0) {
            if (errno == EINTR) {
                continue;
            }
            perror("accept");
            sleep(1);
            continue;
        }
        handle_client(client_fd);
        close(client_fd);
    }

    close(server_fd);
    unlink(socket_path);
    unlink(pid_file);
    reap_children("achost-supervise");
    return 0;
}

static int run_legacy(const char *pid_file, const char *name, int command_index, char **argv) {
    if (install_handlers()) {
        perror("sigaction");
        return 1;
    }
    if (prctl(PR_SET_CHILD_SUBREAPER, 1, 0, 0, 0) < 0) {
        perror("prctl(PR_SET_CHILD_SUBREAPER)");
        return 1;
    }
    if (write_pid_file_for(pid_file, getpid()) < 0) {
        perror("write pid file");
        return 1;
    }

    pid_t child = fork();
    if (child < 0) {
        perror("fork");
        unlink(pid_file);
        return 1;
    }
    if (child == 0) {
        reset_child_signals();
        setpgid(0, 0);
        execvp(argv[command_index], &argv[command_index]);
        perror(argv[command_index]);
        _exit(errno == ENOENT ? 127 : 126);
    }

    setpgid(child, child);
    fprintf(stderr, "%s: supervising pid=%ld\n", name, (long)child);
    int main_status = 0;
    bool main_exited = false;
    for (;;) {
        int status = 0;
        pid_t reaped = waitpid(-1, &status, WNOHANG);
        if (reaped > 0) {
            if (reaped == child) {
                main_status = status;
                main_exited = true;
                fprintf(stderr, "%s: main pid=%ld exited status=%d\n", name, (long)child,
                        exit_code_from_status(status, true));
            }
            continue;
        }
        if (reaped < 0) {
            if (errno == EINTR) {
                continue;
            }
            if (errno == ECHILD) {
                unlink(pid_file);
                return exit_code_from_status(main_status, main_exited);
            }
            perror("waitpid");
        }
        if (pending_signal) {
            int sig = pending_signal;
            pending_signal = 0;
            forward_signal(child, sig);
            fprintf(stderr, "%s: forwarded signal=%d\n", name, sig);
        }
        sleep(1);
    }
}

int main(int argc, char **argv) {
    const char *pid_file = NULL;
    const char *socket_path = NULL;
    const char *name = "achost-supervise";
    const char *launch_chroot = NULL;
    const char *launch_pivot_root = NULL;
    const char *native_root = NULL;
    const char *launch_log_file = NULL;
    int command_index = -1;
    bool server_mode = false;
    bool client_mode = false;
    bool launch_mode = false;

    for (int i = 1; i < argc; i++) {
        if (strcmp(argv[i], "--server") == 0) {
            server_mode = true;
        } else if (strcmp(argv[i], "--client") == 0) {
            client_mode = true;
        } else if (strcmp(argv[i], "--launch") == 0) {
            launch_mode = true;
        } else if (strcmp(argv[i], "--socket") == 0 && i + 1 < argc) {
            socket_path = argv[++i];
        } else if (strcmp(argv[i], "--pid-file") == 0 && i + 1 < argc) {
            pid_file = argv[++i];
        } else if (strcmp(argv[i], "--name") == 0 && i + 1 < argc) {
            name = argv[++i];
        } else if (strcmp(argv[i], "--chroot") == 0 && i + 1 < argc) {
            launch_chroot = argv[++i];
        } else if (strcmp(argv[i], "--pivot-root") == 0 && i + 1 < argc) {
            launch_pivot_root = argv[++i];
        } else if (strcmp(argv[i], "--native-root") == 0 && i + 1 < argc) {
            native_root = argv[++i];
        } else if (strcmp(argv[i], "--log-file") == 0 && i + 1 < argc) {
            launch_log_file = argv[++i];
        } else if (strcmp(argv[i], "--") == 0) {
            command_index = i + 1;
            break;
        } else {
            usage(argv[0]);
            return 2;
        }
    }

    if (launch_mode) {
        if (server_mode || client_mode || pid_file != NULL || socket_path != NULL || native_root != NULL || command_index <= 0 ||
            command_index >= argc || (launch_chroot != NULL && launch_pivot_root != NULL)) {
            usage(argv[0]);
            return 2;
        }
        return run_launch(launch_chroot, launch_pivot_root, launch_log_file, command_index, argv);
    }

    if (server_mode) {
        if (client_mode || socket_path == NULL || pid_file == NULL || launch_chroot != NULL || launch_log_file != NULL ||
            command_index != -1 || (launch_pivot_root != NULL && native_root != NULL)) {
            usage(argv[0]);
            return 2;
        }
        return run_server(socket_path, pid_file, launch_pivot_root, native_root);
    }

    if (client_mode) {
        if (socket_path == NULL || pid_file == NULL || launch_chroot != NULL || launch_pivot_root != NULL || native_root != NULL ||
            launch_log_file != NULL || command_index <= 0 || command_index >= argc) {
            usage(argv[0]);
            return 2;
        }
        return send_client_request(socket_path, pid_file, name, argc - command_index, &argv[command_index]);
    }

    if (launch_chroot != NULL || launch_pivot_root != NULL || native_root != NULL || launch_log_file != NULL || pid_file == NULL ||
        command_index <= 0 || command_index >= argc) {
        usage(argv[0]);
        return 2;
    }
    return run_legacy(pid_file, name, command_index, argv);
}
