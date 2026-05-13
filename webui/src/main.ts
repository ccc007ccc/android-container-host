import { enableEdgeToEdge, exec, toast } from 'kernelsu';
import './style.css';

type Tab = 'dashboard' | 'containers' | 'images' | 'diagnostics' | 'settings';

type StatusData = {
  ok: boolean;
  running: boolean;
  status: string;
  socket: boolean;
  autostart: boolean;
  base_present: boolean;
  data_root: string;
  autostart_file: string;
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

type ImageData = {
  repository: string;
  tag: string;
  id: string;
  size: string;
  created: string;
};

type ApiResponse = Record<string, unknown> & {
  ok?: boolean;
  error?: string;
  output?: string;
  containers?: ContainerData[];
  images?: ImageData[];
};

const DEFAULT_API = '/data/adb/modules/achost-runtime/achost/bin/achost-webui-api.sh';
const app = document.querySelector<HTMLDivElement>('#app')!;

let apiPath = DEFAULT_API;
let activeTab: Tab = 'dashboard';
let statusData: StatusData | null = null;
let containers: ContainerData[] = [];
let images: ImageData[] = [];
let output = '';
let busy = '';
let containerSearch = '';
let formName = '';
let formImage = '';
let formPorts = '';
let formEnv = '';
let formMounts = '';
let formNetwork = 'bridge';
let imageToPull = '';

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

function validCsv(value: string, pattern: RegExp): boolean {
  return !value.trim() || value.split(',').every((item) => pattern.test(item.trim()));
}

function notify(message: string): void {
  try {
    toast(message);
  } catch {}
}

async function loadConfig(): Promise<void> {
  try {
    const response = await fetch('./achost-webui-config.json', { cache: 'no-store' });
    if (!response.ok) return;
    const config = (await response.json()) as { api?: unknown };
    if (typeof config.api === 'string' && config.api.startsWith('/data/adb/modules/')) {
      apiPath = config.api;
    }
  } catch {}
}

async function callApi(action: string, args: string[] = []): Promise<ApiResponse> {
  const command = [apiPath, action, ...args].map(shellQuote).join(' ');
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

function responseText(response: ApiResponse): string {
  if (typeof response.output === 'string') return response.output;
  if (typeof response.error === 'string') return response.error;
  return JSON.stringify(response, null, 2);
}

async function refresh(): Promise<void> {
  busy = 'refresh';
  render();
  try {
    const [status, list, imageList] = await Promise.all([
      callApi('status'),
      callApi('list-containers'),
      callApi('list-images'),
    ]);
    statusData = status as StatusData;
    containers = Array.isArray(list.containers) ? list.containers : [];
    images = Array.isArray(imageList.images) ? imageList.images : [];
    if (status.ok === false) output = responseText(status);
  } catch (error) {
    output = error instanceof Error ? error.message : String(error);
    notify('刷新失败');
  } finally {
    busy = '';
    render();
  }
}

async function runAction(label: string, action: string, args: string[] = [], refreshAfter = true): Promise<void> {
  busy = action;
  render();
  try {
    const response = await callApi(action, args);
    output = responseText(response);
    notify(response.ok === false ? `${label}失败` : `${label}完成`);
    if (refreshAfter) await refresh();
  } catch (error) {
    output = error instanceof Error ? error.message : String(error);
    notify(`${label}失败`);
  } finally {
    busy = '';
    render();
  }
}

async function addContainer(): Promise<void> {
  const name = formName.trim();
  const image = formImage.trim();
  const ports = formPorts.trim();
  const envs = formEnv.trim();
  const mounts = formMounts.trim();
  const network = formNetwork.trim() || 'bridge';
  if (!validName(name)) {
    notify('容器名只能包含字母、数字、点、下划线和短横线');
    return;
  }
  if (!validImage(image)) {
    notify('镜像名包含不支持的字符');
    return;
  }
  if (!validCsv(ports, /^[0-9:/a-z.-]+$/)) {
    notify('端口映射格式不合法');
    return;
  }
  if (!validCsv(envs, /^[A-Za-z_][A-Za-z0-9_=@.,:/+-]*$/)) {
    notify('环境变量格式不合法');
    return;
  }
  if (!validCsv(mounts, /^\/[A-Za-z0-9_./:@,+=-]+:\/[A-Za-z0-9_./:@,+=-]+$/)) {
    notify('挂载格式不合法');
    return;
  }
  await runAction('创建容器', 'add-container', [name, image, ports, envs, mounts, network]);
}

async function pullImage(): Promise<void> {
  const image = imageToPull.trim();
  if (!validImage(image)) {
    notify('镜像名包含不支持的字符');
    return;
  }
  await runAction('拉取镜像', 'pull-image', [image]);
}

async function setAutostart(enabled: boolean): Promise<void> {
  await runAction(enabled ? '开启自启' : '关闭自启', 'set-autostart', [enabled ? 'on' : 'off']);
}

async function runCheck(): Promise<void> {
  activeTab = 'diagnostics';
  await runAction('运行检查', 'check', [], false);
}

function isRunningContainer(item: ContainerData): boolean {
  return item.status.toLowerCase().startsWith('up');
}

function statusPill(): string {
  const running = Boolean(statusData?.running);
  const text = running ? 'Docker 运行中' : 'Docker 已停止';
  return `<span class="pill ${running ? 'ok' : 'stop'}"><span></span>${text}</span>`;
}

function statCard(label: string, value: unknown, tone = ''): string {
  return `<section class="card stat ${tone}"><span>${label}</span><strong>${escapeHtml(value)}</strong></section>`;
}

function detailItem(label: string, value: unknown): string {
  return `<div class="detail-item"><span>${label}</span><strong>${escapeHtml(value || '—')}</strong></div>`;
}

function navItem(tab: Tab, label: string): string {
  return `<button class="nav-item ${activeTab === tab ? 'active' : ''}" data-tab="${tab}">${label}</button>`;
}

function renderDashboard(): string {
  return `
    <section class="grid stats-grid">
      ${statCard('容器总数', statusData?.containers_total ?? '—')}
      ${statCard('运行中', statusData?.containers_running ?? '—', 'green')}
      ${statCard('已停止', statusData?.containers_stopped ?? '—', 'amber')}
      ${statCard('镜像', statusData?.images ?? (images.length || '—'))}
    </section>

    <section class="panel">
      <div class="section-head">
        <div>
          <p class="eyebrow">Runtime</p>
          <h2>运行时概览</h2>
        </div>
        ${statusPill()}
      </div>
      <div class="detail-grid">
        ${detailItem('Docker 版本', statusData?.server_version)}
        ${detailItem('Storage Driver', statusData?.storage_driver)}
        ${detailItem('Cgroup', statusData?.cgroup_version)}
        ${detailItem('dockerd pid', statusData?.dockerd_pid)}
        ${detailItem('containerd pid', statusData?.containerd_pid)}
        ${detailItem('Socket', statusData?.socket ? 'ready' : 'missing')}
        ${detailItem('Base 模块', statusData?.base_present ? 'present' : 'missing')}
        ${detailItem('Data root', statusData?.data_root)}
        ${detailItem('开机自启', statusData?.autostart ? '已开启' : '未开启')}
      </div>
    </section>
  `;
}

function filteredContainers(): ContainerData[] {
  const needle = containerSearch.trim().toLowerCase();
  if (!needle) return containers;
  return containers.filter((item) => [item.name, item.id, item.image, item.status].some((value) => value.toLowerCase().includes(needle)));
}

function renderContainerRows(): string {
  const items = filteredContainers();
  if (!items.length) {
    return '<div class="empty">没有匹配的容器。可以用下方表单创建 detached 容器。</div>';
  }
  return items
    .map((item) => {
      const target = item.name || item.id;
      const running = isRunningContainer(item);
      return `
        <article class="table-row container-row">
          <div>
            <strong>${escapeHtml(item.name || item.id.slice(0, 12))}</strong>
            <small>${escapeHtml(item.id.slice(0, 12))}</small>
          </div>
          <div>
            <span>${escapeHtml(item.image)}</span>
            <small>${escapeHtml(item.created)}</small>
          </div>
          <div><span class="badge ${running ? 'green' : 'slate'}">${escapeHtml(item.status)}</span></div>
          <div class="row-actions">
            <button class="small" data-container-action="start" data-target="${escapeHtml(target)}" ${busy || running ? 'disabled' : ''}>启动</button>
            <button class="small" data-container-action="stop" data-target="${escapeHtml(target)}" ${busy || !running ? 'disabled' : ''}>停止</button>
            <button class="small" data-container-action="restart" data-target="${escapeHtml(target)}" ${busy ? 'disabled' : ''}>重启</button>
            <button class="small ghost" data-container-action="logs" data-target="${escapeHtml(target)}" ${busy ? 'disabled' : ''}>日志</button>
            <button class="small ghost" data-container-action="inspect" data-target="${escapeHtml(target)}" ${busy ? 'disabled' : ''}>Inspect</button>
            <button class="small danger" data-container-action="delete" data-target="${escapeHtml(target)}" ${busy ? 'disabled' : ''}>删除</button>
          </div>
        </article>`;
    })
    .join('');
}

function renderContainers(): string {
  return `
    <section class="panel">
      <div class="section-head split">
        <div>
          <p class="eyebrow">Containers</p>
          <h2>容器管理</h2>
        </div>
        <input class="search" id="container-search" value="${escapeHtml(containerSearch)}" placeholder="搜索名称、镜像或状态" />
      </div>
      <div class="table-list">${renderContainerRows()}</div>
    </section>

    <section class="panel create-panel">
      <div>
        <p class="eyebrow">Run detached</p>
        <h2>创建容器</h2>
      </div>
      <div class="form-grid">
        <label>容器名<input id="name" value="${escapeHtml(formName)}" placeholder="demo-nginx" /></label>
        <label>镜像名<input id="image" value="${escapeHtml(formImage)}" placeholder="nginx:alpine" /></label>
        <label>网络<input id="network" value="${escapeHtml(formNetwork)}" placeholder="bridge" /></label>
        <label>端口映射<input id="ports" value="${escapeHtml(formPorts)}" placeholder="8080:80,8443:443" /></label>
        <label>环境变量<input id="envs" value="${escapeHtml(formEnv)}" placeholder="KEY=value,DEBUG=1" /></label>
        <label>Bind mount<input id="mounts" value="${escapeHtml(formMounts)}" placeholder="/sdcard/www:/usr/share/nginx/html" /></label>
      </div>
      <button data-action="add" ${busy ? 'disabled' : ''}>创建容器</button>
    </section>
  `;
}

function imageName(item: ImageData): string {
  if (item.repository === '<none>' || item.tag === '<none>') return item.id;
  return `${item.repository}:${item.tag}`;
}

function renderImages(): string {
  const rows = images.length
    ? images
        .map(
          (item) => `
          <article class="table-row image-row">
            <div>
              <strong>${escapeHtml(imageName(item))}</strong>
              <small>${escapeHtml(item.id)}</small>
            </div>
            <div><span>${escapeHtml(item.size)}</span><small>${escapeHtml(item.created)}</small></div>
            <div class="row-actions">
              <button class="small danger" data-remove-image="${escapeHtml(imageName(item))}" ${busy ? 'disabled' : ''}>删除镜像</button>
            </div>
          </article>`,
        )
        .join('')
    : '<div class="empty">暂无镜像。可以先拉取镜像，或从命令行 import 本地 rootfs。</div>';

  return `
    <section class="panel create-panel">
      <div>
        <p class="eyebrow">Images</p>
        <h2>拉取镜像</h2>
      </div>
      <label>镜像名<input id="pull-image" value="${escapeHtml(imageToPull)}" placeholder="alpine:latest" /></label>
      <button data-action="pull-image" ${busy ? 'disabled' : ''}>Pull</button>
    </section>
    <section class="panel">
      <div class="section-head">
        <div>
          <p class="eyebrow">Local images</p>
          <h2>本地镜像</h2>
        </div>
      </div>
      <div class="table-list">${rows}</div>
    </section>
  `;
}

function renderDiagnostics(): string {
  return `
    <section class="panel actions-panel">
      <div>
        <p class="eyebrow">Diagnostics</p>
        <h2>诊断与日志</h2>
      </div>
      <div class="button-row">
        <button data-action="check" ${busy ? 'disabled' : ''}>运行 runtime check</button>
        <button data-action="daemon-logs" ${busy ? 'disabled' : ''}>查看 daemon 日志</button>
        <button data-action="refresh" ${busy ? 'disabled' : ''}>刷新状态</button>
      </div>
    </section>
    <section class="panel output-panel">
      <div class="section-head">
        <h2>输出</h2>
      </div>
      <pre>${escapeHtml(output || statusData?.docker_error || '暂无输出')}</pre>
    </section>
  `;
}

function renderSettings(): string {
  const autostart = Boolean(statusData?.autostart);
  return `
    <section class="panel settings-panel">
      <div class="section-head">
        <div>
          <p class="eyebrow">Settings</p>
          <h2>Docker 模块设置</h2>
        </div>
      </div>
      <div class="setting-row">
        <div>
          <strong>开机自启 Docker</strong>
          <small>写入固定配置文件，service.sh 开机时读取。</small>
        </div>
        <button class="switch ${autostart ? 'on' : ''}" data-action="toggle-autostart" ${busy ? 'disabled' : ''}>
          <span></span>${autostart ? '已开启' : '已关闭'}
        </button>
      </div>
      <div class="detail-grid compact">
        ${detailItem('API', apiPath)}
        ${detailItem('Data root', statusData?.data_root)}
        ${detailItem('Autostart file', statusData?.autostart_file)}
        ${detailItem('Base 模块', statusData?.base_present ? 'present' : 'missing')}
      </div>
    </section>
  `;
}

function renderMain(): string {
  if (activeTab === 'containers') return renderContainers();
  if (activeTab === 'images') return renderImages();
  if (activeTab === 'diagnostics') return renderDiagnostics();
  if (activeTab === 'settings') return renderSettings();
  return renderDashboard();
}

function render(): void {
  app.innerHTML = `
    <main class="app-shell">
      <aside class="sidebar">
        <div class="brand">
          <span class="logo">AC</span>
          <div>
            <strong>ACHost</strong>
            <small>Docker Panel</small>
          </div>
        </div>
        <nav>
          ${navItem('dashboard', 'Dashboard')}
          ${navItem('containers', 'Containers')}
          ${navItem('images', 'Images')}
          ${navItem('diagnostics', 'Diagnostics')}
          ${navItem('settings', 'Settings')}
        </nav>
      </aside>

      <section class="content">
        <header class="topbar">
          <div>
            <p class="eyebrow">KernelSU Module WebUI</p>
            <h1>Docker 管理面板</h1>
            <p class="subtle">管理 Docker 状态、容器、镜像、诊断日志和开机自启。</p>
          </div>
          <div class="top-actions">
            ${statusPill()}
            <button data-action="refresh" ${busy ? 'disabled' : ''}>${busy === 'refresh' ? '刷新中…' : '刷新'}</button>
            <button data-action="start" ${busy || statusData?.running ? 'disabled' : ''}>启动</button>
            <button class="danger" data-action="stop" ${busy || !statusData?.running ? 'disabled' : ''}>停止</button>
          </div>
        </header>
        ${renderMain()}
      </section>
    </main>
  `;

  app.querySelectorAll<HTMLButtonElement>('[data-tab]').forEach((button) => {
    button.addEventListener('click', () => {
      activeTab = button.dataset.tab as Tab;
      render();
    });
  });
  app.querySelector<HTMLButtonElement>('[data-action="refresh"]')?.addEventListener('click', () => refresh());
  app.querySelector<HTMLButtonElement>('[data-action="start"]')?.addEventListener('click', () => runAction('启动 Docker', 'start-docker'));
  app.querySelector<HTMLButtonElement>('[data-action="stop"]')?.addEventListener('click', () => runAction('停止 Docker', 'stop-docker'));
  app.querySelector<HTMLButtonElement>('[data-action="check"]')?.addEventListener('click', () => runCheck());
  app.querySelector<HTMLButtonElement>('[data-action="daemon-logs"]')?.addEventListener('click', () => runAction('读取日志', 'daemon-logs', [], false));
  app.querySelector<HTMLButtonElement>('[data-action="add"]')?.addEventListener('click', () => addContainer());
  app.querySelector<HTMLButtonElement>('[data-action="pull-image"]')?.addEventListener('click', () => pullImage());
  app.querySelector<HTMLButtonElement>('[data-action="toggle-autostart"]')?.addEventListener('click', () => setAutostart(!statusData?.autostart));

  app.querySelector<HTMLInputElement>('#container-search')?.addEventListener('input', (event) => {
    containerSearch = (event.target as HTMLInputElement).value;
    render();
  });
  app.querySelector<HTMLInputElement>('#name')?.addEventListener('input', (event) => {
    formName = (event.target as HTMLInputElement).value;
  });
  app.querySelector<HTMLInputElement>('#image')?.addEventListener('input', (event) => {
    formImage = (event.target as HTMLInputElement).value;
  });
  app.querySelector<HTMLInputElement>('#network')?.addEventListener('input', (event) => {
    formNetwork = (event.target as HTMLInputElement).value;
  });
  app.querySelector<HTMLInputElement>('#ports')?.addEventListener('input', (event) => {
    formPorts = (event.target as HTMLInputElement).value;
  });
  app.querySelector<HTMLInputElement>('#envs')?.addEventListener('input', (event) => {
    formEnv = (event.target as HTMLInputElement).value;
  });
  app.querySelector<HTMLInputElement>('#mounts')?.addEventListener('input', (event) => {
    formMounts = (event.target as HTMLInputElement).value;
  });
  app.querySelector<HTMLInputElement>('#pull-image')?.addEventListener('input', (event) => {
    imageToPull = (event.target as HTMLInputElement).value;
  });

  app.querySelectorAll<HTMLButtonElement>('[data-container-action]').forEach((button) => {
    button.addEventListener('click', () => {
      const action = button.dataset.containerAction || '';
      const target = button.dataset.target || '';
      if (!target) return;
      if (action === 'logs') runAction('读取容器日志', 'container-logs', [target], false);
      if (action === 'inspect') runAction('Inspect 容器', 'inspect-container', [target], false);
      if (action === 'delete') runAction('删除容器', 'delete-container', [target]);
      if (action === 'start') runAction('启动容器', 'start-container', [target]);
      if (action === 'stop') runAction('停止容器', 'stop-container', [target]);
      if (action === 'restart') runAction('重启容器', 'restart-container', [target]);
    });
  });
  app.querySelectorAll<HTMLButtonElement>('[data-remove-image]').forEach((button) => {
    button.addEventListener('click', () => {
      const target = button.dataset.removeImage || '';
      if (target) runAction('删除镜像', 'remove-image', [target]);
    });
  });
}

render();
loadConfig().finally(() => refresh());
