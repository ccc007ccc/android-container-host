import { enableEdgeToEdge, exec, toast } from 'kernelsu';
import './style.css';

type StatusData = {
  ok: boolean;
  running: boolean;
  status: string;
  socket: boolean;
  dockerd_pid: string;
  containerd_pid: string;
  cgroup_version: string;
  storage_driver: string;
  server_version: string;
  containers_total: number;
  containers_running: number;
  containers_stopped: number;
  images: number;
  error?: string;
  docker_error?: string;
};

type ContainerData = {
  id: string;
  name: string;
  image: string;
  status: string;
  created: string;
};

type ApiResponse = Record<string, unknown> & {
  ok?: boolean;
  error?: string;
  output?: string;
  containers?: ContainerData[];
};

const API = '/data/adb/modules/achost-runtime/achost/bin/achost-webui-api.sh';
const app = document.querySelector<HTMLDivElement>('#app')!;

let statusData: StatusData | null = null;
let containers: ContainerData[] = [];
let checkOutput = '';
let busy = '';
let formName = '';
let formImage = '';

try {
  enableEdgeToEdge(true);
} catch {}

function escapeHtml(value: unknown): string {
  return String(value ?? '')
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
    .replace(/'/g, '&#39;');
}

function shellQuote(value: string): string {
  return `'${value.replace(/'/g, `'\\''`)}'`;
}

function validName(value: string): boolean {
  return /^[A-Za-z0-9_.-]+$/.test(value);
}

function validImage(value: string): boolean {
  return /^[A-Za-z0-9_./:@-]+$/.test(value);
}

function notify(message: string): void {
  try {
    toast(message);
  } catch {}
}

async function callApi(action: string, args: string[] = []): Promise<ApiResponse> {
  const command = [API, action, ...args.map(shellQuote)].join(' ');
  const result = await exec(command);
  const text = result.stdout.trim() || result.stderr.trim();
  if (!text) {
    return { ok: false, error: `命令没有输出，errno=${result.errno}` };
  }
  try {
    const parsed = JSON.parse(text) as ApiResponse;
    if (result.errno !== 0 && parsed.ok !== false) {
      parsed.ok = false;
      parsed.error = `errno=${result.errno}`;
    }
    return parsed;
  } catch {
    return { ok: false, error: text };
  }
}

async function refresh(): Promise<void> {
  busy = 'refresh';
  render();
  try {
    const [status, list] = await Promise.all([callApi('status'), callApi('list-containers')]);
    statusData = status as StatusData;
    containers = Array.isArray(list.containers) ? list.containers : [];
  } catch (error) {
    checkOutput = error instanceof Error ? error.message : String(error);
    notify('刷新失败');
  } finally {
    busy = '';
    render();
  }
}

async function runAction(label: string, action: string, args: string[] = []): Promise<void> {
  busy = action;
  render();
  try {
    const response = await callApi(action, args);
    checkOutput = String(response.output || response.error || '完成');
    notify(response.ok === false ? `${label}失败` : `${label}完成`);
    await refresh();
  } catch (error) {
    checkOutput = error instanceof Error ? error.message : String(error);
    notify(`${label}失败`);
  } finally {
    busy = '';
    render();
  }
}

async function addContainer(): Promise<void> {
  const name = formName.trim();
  const image = formImage.trim();
  if (!validName(name)) {
    notify('容器名只能包含字母、数字、点、下划线和短横线');
    return;
  }
  if (!validImage(image)) {
    notify('镜像名包含不支持的字符');
    return;
  }
  await runAction('新增容器', 'add-container', [name, image]);
}

async function runCheck(): Promise<void> {
  busy = 'check';
  render();
  try {
    const response = await callApi('check');
    checkOutput = String(response.output || response.error || '检查完成');
    notify(response.ok === false ? '检查发现问题' : '检查通过');
  } catch (error) {
    checkOutput = error instanceof Error ? error.message : String(error);
    notify('检查失败');
  } finally {
    busy = '';
    render();
  }
}

function statCard(label: string, value: unknown, tone = ''): string {
  return `<section class="card stat ${tone}"><span>${label}</span><strong>${escapeHtml(value)}</strong></section>`;
}

function statusPill(): string {
  const running = Boolean(statusData?.running);
  const text = running ? 'Docker 运行中' : 'Docker 已停止';
  return `<span class="pill ${running ? 'ok' : 'stop'}"><span></span>${text}</span>`;
}

function renderContainers(): string {
  if (!containers.length) {
    return '<div class="empty">暂无容器。可以在下方输入名称和镜像创建一个 detached bridge 容器。</div>';
  }
  return containers
    .map(
      (item) => `
        <article class="container-row">
          <div>
            <strong>${escapeHtml(item.name || item.id.slice(0, 12))}</strong>
            <small>${escapeHtml(item.id.slice(0, 12))}</small>
          </div>
          <div>
            <span>${escapeHtml(item.image)}</span>
            <small>${escapeHtml(item.created)}</small>
          </div>
          <div><span class="status-text">${escapeHtml(item.status)}</span></div>
          <button class="danger" data-delete="${escapeHtml(item.name || item.id)}">删除</button>
        </article>`,
    )
    .join('');
}

function render(): void {
  const running = Boolean(statusData?.running);
  app.innerHTML = `
    <main class="shell">
      <header class="hero">
        <div>
          <p class="eyebrow">KernelSU Module WebUI</p>
          <h1>ACHost Docker</h1>
          <p class="subtle">查看 Docker 状态、容器数量，并执行受限的新增、删除和检查操作。</p>
        </div>
        ${statusPill()}
      </header>

      <section class="actions panel">
        <button data-action="refresh" ${busy ? 'disabled' : ''}>${busy === 'refresh' ? '刷新中…' : '刷新'}</button>
        <button data-action="start" ${busy || running ? 'disabled' : ''}>启动 Docker</button>
        <button data-action="stop" ${busy || !running ? 'disabled' : ''}>停止 Docker</button>
        <button data-action="check" ${busy ? 'disabled' : ''}>运行检查</button>
      </section>

      <section class="grid stats-grid">
        ${statCard('容器总数', statusData?.containers_total ?? '—')}
        ${statCard('运行中', statusData?.containers_running ?? '—', 'green')}
        ${statCard('已停止', statusData?.containers_stopped ?? '—', 'amber')}
        ${statCard('镜像', statusData?.images ?? '—')}
      </section>

      <section class="grid detail-grid">
        ${statCard('dockerd pid', statusData?.dockerd_pid || '—')}
        ${statCard('containerd pid', statusData?.containerd_pid || '—')}
        ${statCard('socket', statusData?.socket ? 'ready' : 'missing', statusData?.socket ? 'green' : 'amber')}
        ${statCard('cgroup', statusData?.cgroup_version || '—')}
        ${statCard('Docker', statusData?.server_version || '—')}
        ${statCard('Storage', statusData?.storage_driver || '—')}
      </section>

      <section class="panel">
        <div class="section-head">
          <div>
            <p class="eyebrow">Containers</p>
            <h2>容器列表</h2>
          </div>
        </div>
        <div class="container-list">${renderContainers()}</div>
      </section>

      <section class="panel add-panel">
        <div>
          <p class="eyebrow">Run detached</p>
          <h2>新增容器</h2>
        </div>
        <label>容器名<input id="name" value="${escapeHtml(formName)}" placeholder="demo-nginx" /></label>
        <label>镜像名<input id="image" value="${escapeHtml(formImage)}" placeholder="nginx:alpine" /></label>
        <button data-action="add" ${busy ? 'disabled' : ''}>创建</button>
      </section>

      <section class="panel output-panel">
        <div class="section-head">
          <h2>检查输出</h2>
        </div>
        <pre>${escapeHtml(checkOutput || statusData?.docker_error || '暂无输出')}</pre>
      </section>
    </main>
  `;

  app.querySelector<HTMLButtonElement>('[data-action="refresh"]')?.addEventListener('click', () => refresh());
  app.querySelector<HTMLButtonElement>('[data-action="start"]')?.addEventListener('click', () => runAction('启动 Docker', 'start-docker'));
  app.querySelector<HTMLButtonElement>('[data-action="stop"]')?.addEventListener('click', () => runAction('停止 Docker', 'stop-docker'));
  app.querySelector<HTMLButtonElement>('[data-action="check"]')?.addEventListener('click', () => runCheck());
  app.querySelector<HTMLButtonElement>('[data-action="add"]')?.addEventListener('click', () => addContainer());
  app.querySelector<HTMLInputElement>('#name')?.addEventListener('input', (event) => {
    formName = (event.target as HTMLInputElement).value;
  });
  app.querySelector<HTMLInputElement>('#image')?.addEventListener('input', (event) => {
    formImage = (event.target as HTMLInputElement).value;
  });
  app.querySelectorAll<HTMLButtonElement>('[data-delete]').forEach((button) => {
    button.addEventListener('click', () => {
      const target = button.dataset.delete || '';
      if (target) runAction('删除容器', 'delete-container', [target]);
    });
  });
}

render();
refresh();
