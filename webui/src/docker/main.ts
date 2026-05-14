import { enableEdgeToEdge, exec, toast } from 'kernelsu';
import '../style.css';

type Tab = 'dashboard' | 'containers' | 'images' | 'diagnostics' | 'settings';

type StatusData = {
  ok?: boolean;
  running?: boolean;
  socket?: boolean;
  autostart?: boolean;
  base_present?: boolean;
  data_root?: string;
  autostart_file?: string;
  dockerd_pid?: string;
  containerd_pid?: string;
  cgroup_version?: string;
  configured_cgroup_mode?: string;
  cgroup_mount?: string;
  runtime_mode?: string;
  dns_servers?: string;
  resolv_conf?: string;
  resolv_nameservers?: string;
  bridge?: string;
  bridge_subnet?: string;
  bridge_route?: string;
  route_status?: string;
  return_policy_rule?: string;
  source_policy_rule?: string;
  uplink?: string;
  storage_driver?: string;
  server_version?: string;
  containers_total?: number;
  containers_running?: number;
  containers_stopped?: number;
  images?: number;
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
  containers?: unknown[];
  images?: unknown[];
};

type ExecResult = {
  errno: number;
  stdout: string;
  stderr: string;
};

type WebuiConfig = {
  api?: unknown;
  moduleTarget?: unknown;
  moduleId?: unknown;
};

type ConfirmIntent =
  | { kind: 'delete-container'; target: string }
  | { kind: 'remove-image'; target: string };

type SheetState =
  | { kind: 'none' }
  | { kind: 'docker-run' }
  | { kind: 'image-pull' }
  | { kind: 'container-actions'; target: string }
  | { kind: 'image-actions'; target: string }
  | { kind: 'output'; title: string }
  | { kind: 'confirm'; title: string; message: string; confirmLabel: string; intent: ConfirmIntent };

type FocusSnapshot = {
  id: string;
  selectionStart: number | null;
  selectionEnd: number | null;
};

type NavItem = {
  tab: Tab;
  label: string;
};

const DEFAULT_API = '/data/adb/modules/achost-docker/achost/bin/achost-webui-api.sh';
const app = document.querySelector<HTMLDivElement>('#app')!;

const state = {
  apiPath: DEFAULT_API,
  moduleId: 'achost-docker',
  activeTab: 'dashboard' as Tab,
  statusData: null as StatusData | null,
  containers: [] as ContainerData[],
  images: [] as ImageData[],
  output: '',
  busy: '',
  ui: {
    sheet: { kind: 'none' } as SheetState,
    expanded: {} as Record<string, boolean>,
  },
  forms: {
    containerSearch: '',
    dockerName: '',
    dockerImage: '',
    dockerPorts: '',
    dockerEnv: '',
    dockerMounts: '',
    dockerNetwork: 'bridge',
    imageToPull: '',
  },
};

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

function notify(message: string): void {
  try {
    toast(message);
  } catch {}
}

function validName(value: string): boolean {
  return /^[A-Za-z0-9_.-]+$/.test(value) && value !== '.' && value !== '..' && !value.includes('..');
}

function validImage(value: string): boolean {
  return /^[A-Za-z0-9_./:@-]+$/.test(value);
}

function validCsv(value: string, pattern: RegExp): boolean {
  return !value.trim() || value.split(',').every((item) => pattern.test(item.trim()));
}

function storageKey(name: string): string {
  return `achost-webui:docker:${state.moduleId}:${name}`;
}

function readStorage(key: string): string | null {
  try {
    return localStorage.getItem(key);
  } catch {
    return null;
  }
}

function writeStorage(key: string, value: string): void {
  try {
    localStorage.setItem(key, value);
  } catch {}
}

function availableTabs(): NavItem[] {
  return [
    { tab: 'dashboard', label: '概览' },
    { tab: 'containers', label: '容器' },
    { tab: 'images', label: '镜像' },
    { tab: 'diagnostics', label: '诊断' },
    { tab: 'settings', label: '设置' },
  ];
}

function isValidTab(tab: string): tab is Tab {
  return availableTabs().some((item) => item.tab === tab);
}

function applyConfig(config: WebuiConfig): void {
  if (typeof config.api === 'string' && config.api.startsWith('/data/adb/modules/')) {
    state.apiPath = config.api;
  }
  if (typeof config.moduleId === 'string' && config.moduleId) {
    state.moduleId = config.moduleId;
  }
}

function readConfig(): void {
  const meta = document.querySelector<HTMLMetaElement>('meta[name="achost-webui-config"]');
  const content = meta?.content || '';
  if (!content) return;
  try {
    applyConfig(JSON.parse(content) as WebuiConfig);
  } catch {}
}

function restorePersistedUi(): void {
  const tab = readStorage(storageKey('active-tab'));
  state.activeTab = tab && isValidTab(tab) ? tab : 'dashboard';
  const expanded = readStorage(storageKey('expanded'));
  if (!expanded) return;
  try {
    const parsed = JSON.parse(expanded) as Record<string, unknown>;
    state.ui.expanded = Object.fromEntries(
      Object.entries(parsed).filter(([, value]) => typeof value === 'boolean'),
    ) as Record<string, boolean>;
  } catch {
    state.ui.expanded = {};
  }
}

function persistActiveTab(): void {
  writeStorage(storageKey('active-tab'), state.activeTab);
}

function persistExpanded(): void {
  writeStorage(storageKey('expanded'), JSON.stringify(state.ui.expanded));
}

async function callApi(action: string, args: string[] = []): Promise<ApiResponse> {
  const command = [state.apiPath, action, ...args].map(shellQuote).join(' ');
  const result = (await exec(command)) as ExecResult;
  return parseApiResult(result);
}

function parseApiResult(result: ExecResult): ApiResponse {
  const text = result.stdout.trim() || result.stderr.trim();
  if (!text) return { ok: false, error: `命令没有输出，errno=${result.errno}` };
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

function normalizeContainers(value: unknown): ContainerData[] {
  if (!Array.isArray(value)) return [];
  return value
    .map((item) => {
      const row = item as Record<string, unknown>;
      return {
        id: String(row.id ?? ''),
        name: String(row.name ?? ''),
        image: String(row.image ?? ''),
        status: String(row.status ?? ''),
        created: String(row.created ?? ''),
      };
    })
    .filter((item) => item.id || item.name);
}

function normalizeImages(value: unknown): ImageData[] {
  if (!Array.isArray(value)) return [];
  return value
    .map((item) => {
      const row = item as Record<string, unknown>;
      return {
        repository: String(row.repository ?? ''),
        tag: String(row.tag ?? ''),
        id: String(row.id ?? ''),
        size: String(row.size ?? ''),
        created: String(row.created ?? ''),
      };
    })
    .filter((item) => item.id || item.repository);
}

async function loadData(): Promise<void> {
  const [status, list, imageList] = await Promise.all([
    callApi('status'),
    callApi('list-containers'),
    callApi('list-images'),
  ]);
  state.statusData = status as StatusData;
  state.containers = normalizeContainers(list.containers);
  state.images = normalizeImages(imageList.images);
  if (status.ok === false) state.output = responseText(status);
  else if (list.ok === false) state.output = responseText(list);
  else if (imageList.ok === false) state.output = responseText(imageList);
}

async function refresh(): Promise<void> {
  state.busy = 'refresh';
  render();
  try {
    await loadData();
  } catch (error) {
    state.output = error instanceof Error ? error.message : String(error);
    notify('刷新失败');
  } finally {
    state.busy = '';
    render();
  }
}

async function runAction(
  label: string,
  action: string,
  args: string[] = [],
  refreshAfter = true,
  options: { showOutput?: boolean } = {},
): Promise<void> {
  state.busy = action;
  render();
  try {
    const response = await callApi(action, args);
    const failed = response.ok === false;
    state.output = responseText(response);
    notify(failed ? `${label}失败` : `${label}完成`);
    if (refreshAfter) await loadData();
    if (failed || options.showOutput || !refreshAfter) {
      state.ui.sheet = { kind: 'output', title: failed ? `${label}失败` : label };
    } else if (state.ui.sheet.kind !== 'none') {
      state.ui.sheet = { kind: 'none' };
    }
  } catch (error) {
    state.output = error instanceof Error ? error.message : String(error);
    state.ui.sheet = { kind: 'output', title: `${label}失败` };
    notify(`${label}失败`);
  } finally {
    state.busy = '';
    render();
  }
}

async function addContainer(): Promise<void> {
  const name = state.forms.dockerName.trim();
  const image = state.forms.dockerImage.trim();
  const ports = state.forms.dockerPorts.trim();
  const envs = state.forms.dockerEnv.trim();
  const mounts = state.forms.dockerMounts.trim();
  const network = state.forms.dockerNetwork.trim() || 'bridge';
  if (!validName(name)) return notify('容器名只能包含字母、数字、点、下划线和短横线');
  if (!validImage(image)) return notify('镜像名包含不支持的字符');
  if (!validCsv(ports, /^[0-9:/a-z.-]+$/)) return notify('端口映射格式不合法');
  if (!validCsv(envs, /^[A-Za-z_][A-Za-z0-9_=@.,:/+-]*$/)) return notify('环境变量格式不合法');
  if (!validCsv(mounts, /^\/[A-Za-z0-9_./:@,+=-]+:\/[A-Za-z0-9_./:@,+=-]+$/)) return notify('挂载格式不合法');
  await runAction('创建容器', 'add-container', [name, image, ports, envs, mounts, network]);
}

async function pullImage(): Promise<void> {
  const image = state.forms.imageToPull.trim();
  if (!validImage(image)) return notify('镜像名包含不支持的字符');
  await runAction('拉取镜像', 'pull-image', [image]);
}

async function setAutostart(enabled: boolean): Promise<void> {
  await runAction(enabled ? '开启自启' : '关闭自启', 'set-autostart', [enabled ? 'on' : 'off']);
}

async function runCheck(): Promise<void> {
  state.activeTab = 'diagnostics';
  persistActiveTab();
  await runAction('运行检查', 'check', [], false, { showOutput: true });
}

function isRunningContainer(item: ContainerData): boolean {
  return item.status.toLowerCase().startsWith('up');
}

function statusPill(): string {
  const running = Boolean(state.statusData?.running);
  return `<span class="pill ${running ? 'ok' : 'stop'}"><span></span>${running ? 'Docker 运行' : 'Docker 停止'}</span>`;
}

function metricCard(label: string, value: unknown, tone = ''): string {
  return `<article class="metric-card ${tone}"><span>${escapeHtml(label)}</span><strong>${escapeHtml(value)}</strong></article>`;
}

function metricStrip(items: Array<[string, unknown, string?]>): string {
  return `<section class="metric-strip">${items.map(([label, value, tone]) => metricCard(label, value, tone)).join('')}</section>`;
}

function detailItem(label: string, value: unknown): string {
  return `<div class="detail-item"><span>${escapeHtml(label)}</span><strong>${escapeHtml(value || '—')}</strong></div>`;
}

function detailGrid(items: Array<[string, unknown]>): string {
  return `<div class="detail-grid">${items.map(([label, value]) => detailItem(label, value)).join('')}</div>`;
}

function isExpanded(id: string, fallback = false): boolean {
  return state.ui.expanded[id] ?? fallback;
}

function accordion(id: string, title: string, subtitle: string, body: string, fallbackOpen = false, badge = ''): string {
  const open = isExpanded(id, fallbackOpen);
  return `
    <section class="panel accordion ${open ? 'open' : ''}">
      <button type="button" class="accordion-trigger" data-accordion="${escapeHtml(id)}" aria-expanded="${open ? 'true' : 'false'}">
        <span>
          <small>${escapeHtml(subtitle)}</small>
          <strong>${escapeHtml(title)}</strong>
        </span>
        <span class="accordion-side">${badge}<span class="chevron">${open ? '收起' : '展开'}</span></span>
      </button>
      ${open ? `<div class="accordion-body">${body}</div>` : ''}
    </section>`;
}

function pageIntro(title: string): string {
  return `
    <section class="page-intro">
      <div>
        <p class="eyebrow">KernelSU Module WebUI</p>
        <h1>${escapeHtml(title)}</h1>
      </div>
      <div class="top-actions">${renderTopActions()}</div>
    </section>`;
}

function actionPanel(title: string, body: string): string {
  return `
    <section class="panel command-panel">
      <div>
        <p class="eyebrow">Control</p>
        <h2>${escapeHtml(title)}</h2>
      </div>
      <div class="button-row">${body}</div>
    </section>`;
}

function button(label: string, action: string, className = '', disabled = false): string {
  return `<button type="button" class="${escapeHtml(className)}" data-action="${escapeHtml(action)}" ${disabled ? 'disabled' : ''}>${escapeHtml(label)}</button>`;
}

function renderDashboard(): string {
  return `
    ${metricStrip([
      ['容器', state.statusData?.containers_total ?? '—'],
      ['运行', state.statusData?.containers_running ?? '—', 'success'],
      ['停止', state.statusData?.containers_stopped ?? '—', 'warning'],
      ['镜像', state.statusData?.images ?? (state.images.length || '—')],
    ])}
    ${actionPanel(
      'Docker 控制台',
      `${button('刷新', 'refresh', 'ghost', Boolean(state.busy))}${button('启动 Docker', 'start', '', Boolean(state.busy || state.statusData?.running))}${button('停止 Docker', 'stop', 'danger', Boolean(state.busy || !state.statusData?.running))}${button('运行检查', 'check', 'ghost', Boolean(state.busy))}`,
    )}
    ${accordion(
      'docker-runtime',
      '运行时详情',
      'Runtime',
      detailGrid([
        ['Docker 版本', state.statusData?.server_version],
        ['Storage Driver', state.statusData?.storage_driver],
        ['Cgroup', state.statusData?.cgroup_version],
        ['dockerd pid', state.statusData?.dockerd_pid],
        ['containerd pid', state.statusData?.containerd_pid],
        ['Socket', state.statusData?.socket ? 'ready' : 'missing'],
        ['Base 模块', state.statusData?.base_present ? 'present' : 'missing'],
        ['Data root', state.statusData?.data_root],
        ['开机自启', state.statusData?.autostart ? '已开启' : '未开启'],
      ]),
      false,
      statusPill(),
    )}
    ${renderOutputAccordion('docker-output')}
  `;
}

function filteredContainers(): ContainerData[] {
  const needle = state.forms.containerSearch.trim().toLowerCase();
  if (!needle) return state.containers;
  return state.containers.filter((item) =>
    [item.name, item.id, item.image, item.status].some((value) => value.toLowerCase().includes(needle)),
  );
}

function renderSearchPanel(title: string, placeholder: string, extraAction = ''): string {
  return `
    <section class="panel list-tools">
      <div>
        <p class="eyebrow">Inventory</p>
        <h2>${escapeHtml(title)}</h2>
      </div>
      <div class="toolbar-row">
        <input class="search" id="container-search" value="${escapeHtml(state.forms.containerSearch)}" placeholder="${escapeHtml(placeholder)}" autocomplete="off" />
        ${extraAction}
      </div>
    </section>`;
}

function renderContainerRows(): string {
  const items = filteredContainers();
  if (!items.length) return '<div class="empty">没有匹配的容器。</div>';
  return items
    .map((item) => {
      const target = item.id || item.name;
      const running = isRunningContainer(item);
      const primaryAction = running ? 'stop' : 'start';
      return `
        <article class="entity-card">
          <div class="entity-main">
            <strong title="${escapeHtml(item.name || item.id)}">${escapeHtml(item.name || '(无名称)')}</strong>
            <small title="${escapeHtml(item.id)}">${escapeHtml(item.id)}</small>
          </div>
          <div class="entity-meta">
            <span title="${escapeHtml(item.image)}">${escapeHtml(item.image)}</span>
            <small title="${escapeHtml(item.created)}">创建: ${escapeHtml(item.created || '—')}</small>
          </div>
          <div class="entity-status"><span class="badge ${running ? 'green' : 'slate'}">${escapeHtml(item.status)}</span></div>
          <div class="entity-actions">
            <button type="button" class="small" data-container-action="${primaryAction}" data-target="${escapeHtml(target)}" ${state.busy ? 'disabled' : ''}>${running ? '停止' : '启动'}</button>
            <button type="button" class="small ghost" data-container-action="more" data-target="${escapeHtml(target)}" ${state.busy ? 'disabled' : ''}>更多</button>
          </div>
        </article>`;
    })
    .join('');
}

function renderContainers(): string {
  return `
    ${renderSearchPanel(
      '容器管理',
      '搜索名称、镜像或状态',
      `<button type="button" data-sheet="docker-run" ${state.busy ? 'disabled' : ''}>创建容器</button>`,
    )}
    <section class="entity-list">${renderContainerRows()}</section>
    ${renderOutputAccordion('container-output')}
  `;
}

function imageName(item: ImageData): string {
  if (item.repository === '<none>' || item.tag === '<none>') return item.id;
  return `${item.repository}:${item.tag}`;
}

function renderImages(): string {
  const rows = state.images.length
    ? state.images
        .map(
          (item) => `
            <article class="entity-card image-card">
              <div class="entity-main">
                <strong>${escapeHtml(imageName(item))}</strong>
                <small>${escapeHtml(item.id)}</small>
              </div>
              <div class="entity-meta"><span>${escapeHtml(item.size)}</span><small>${escapeHtml(item.created)}</small></div>
              <div class="entity-actions">
                <button type="button" class="small ghost" data-image-action="more" data-target="${escapeHtml(item.id)}" ${state.busy ? 'disabled' : ''}>更多</button>
              </div>
            </article>`,
        )
        .join('')
    : '<div class="empty">暂无镜像。可以先拉取镜像，或从命令行导入本地镜像。</div>';

  return `
    <section class="panel list-tools">
      <div>
        <p class="eyebrow">Images</p>
        <h2>本地镜像</h2>
      </div>
      <button type="button" data-sheet="image-pull" ${state.busy ? 'disabled' : ''}>拉取镜像</button>
    </section>
    <section class="entity-list">${rows}</section>
    ${renderOutputAccordion('image-output')}
  `;
}

function renderDiagnostics(): string {
  return `
    ${actionPanel(
      '诊断与日志',
      `${button('运行 runtime check', 'check', '', Boolean(state.busy))}${button('查看 daemon 日志', 'daemon-logs', 'ghost', Boolean(state.busy))}${button('刷新状态', 'refresh', 'ghost', Boolean(state.busy))}`,
    )}
    ${accordion(
      'docker-compat',
      '非常规环境状态',
      'Android compatibility',
      detailGrid([
        ['Runtime mode', state.statusData?.runtime_mode],
        ['Cgroup mode', state.statusData?.configured_cgroup_mode || state.statusData?.cgroup_version],
        ['Host cgroup', state.statusData?.cgroup_mount],
        ['DNS servers', state.statusData?.dns_servers || state.statusData?.resolv_nameservers],
        ['resolv.conf', state.statusData?.resolv_conf],
        ['resolv nameservers', state.statusData?.resolv_nameservers],
        ['Bridge', state.statusData?.bridge],
        ['Bridge subnet', state.statusData?.bridge_subnet],
        ['Bridge route', state.statusData?.bridge_route],
        ['Return policy', state.statusData?.return_policy_rule],
        ['Source policy', state.statusData?.source_policy_rule],
        ['Uplink', state.statusData?.uplink],
      ]),
      false,
      `<span class="badge ${state.statusData?.route_status === 'ok' ? 'green' : 'slate'}">路由 ${escapeHtml(state.statusData?.route_status || 'unknown')}</span>`,
    )}
    ${renderOutputAccordion('diagnostics-output', state.statusData?.docker_error)}
  `;
}

function renderSettings(): string {
  const autostart = Boolean(state.statusData?.autostart);
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
        <button type="button" class="switch ${autostart ? 'on' : ''}" data-action="toggle-autostart" ${state.busy ? 'disabled' : ''}>
          <span></span>${autostart ? '已开启' : '已关闭'}
        </button>
      </div>
    </section>
    ${accordion(
      'docker-settings-paths',
      '路径与 API',
      'Advanced',
      detailGrid([
        ['API', state.apiPath],
        ['Data root', state.statusData?.data_root],
        ['Autostart file', state.statusData?.autostart_file],
        ['Base 模块', state.statusData?.base_present ? 'present' : 'missing'],
      ]),
      false,
    )}
    ${renderOutputAccordion('settings-output')}
  `;
}

function renderOutputAccordion(id: string, fallback?: unknown): string {
  const text = state.output || fallback || '';
  if (!text) return '';
  return accordion(
    id,
    '最近输出',
    'Output',
    `<pre class="output-pre">${escapeHtml(text)}</pre><div class="button-row compact"><button type="button" class="ghost" data-action="output">打开输出面板</button></div>`,
    false,
  );
}

function renderMain(): string {
  if (state.activeTab === 'containers') return renderContainers();
  if (state.activeTab === 'images') return renderImages();
  if (state.activeTab === 'diagnostics') return renderDiagnostics();
  if (state.activeTab === 'settings') return renderSettings();
  return renderDashboard();
}

function renderNav(className: string): string {
  return `<nav class="${escapeHtml(className)}" aria-label="主导航">${availableTabs()
    .map(
      (item) => `
        <button type="button" class="nav-item ${state.activeTab === item.tab ? 'active' : ''}" data-tab="${item.tab}" ${state.activeTab === item.tab ? 'aria-current="page"' : ''}>
          <span>${escapeHtml(item.label)}</span>
        </button>`,
    )
    .join('')}</nav>`;
}

function renderTopActions(): string {
  const outputButton = state.output ? button('输出', 'output', 'ghost', false) : '';
  return `${statusPill()}${button(state.busy === 'refresh' ? '刷新中…' : '刷新', 'refresh', 'ghost', Boolean(state.busy))}${button('启动', 'start', '', Boolean(state.busy || state.statusData?.running))}${button('停止', 'stop', 'danger', Boolean(state.busy || !state.statusData?.running))}${outputButton}`;
}

function renderBrand(): string {
  return `
    <div class="brand">
      <span class="logo">AC</span>
      <div>
        <strong>ACHost</strong>
        <small>Docker Panel</small>
      </div>
    </div>`;
}

function renderShell(): string {
  return `
    <main class="app-shell docker-shell">
      <aside class="side-rail">
        ${renderBrand()}
        ${renderNav('side-nav')}
      </aside>
      <section class="workspace">
        <header class="mobile-header">
          ${renderBrand()}
          ${statusPill()}
        </header>
        ${pageIntro('Docker 管理面板')}
        <div class="page-stack">${renderMain()}</div>
      </section>
      ${renderNav('bottom-nav')}
      ${renderSheet()}
    </main>`;
}

function renderSheet(): string {
  if (state.ui.sheet.kind === 'none') return '';
  let title = '';
  let body = '';
  let footer = '';
  const sheet = state.ui.sheet;
  if (sheet.kind === 'docker-run') {
    title = '创建 Docker 容器';
    body = renderDockerRunForm();
    footer = `${button('创建容器', 'add', '', Boolean(state.busy))}${button('取消', 'close-sheet', 'ghost', false)}`;
  } else if (sheet.kind === 'image-pull') {
    title = '拉取镜像';
    body = `<label>镜像名<input id="pull-image" value="${escapeHtml(state.forms.imageToPull)}" placeholder="alpine:latest" autofocus /></label>`;
    footer = `${button('Pull', 'pull-image', '', Boolean(state.busy))}${button('取消', 'close-sheet', 'ghost', false)}`;
  } else if (sheet.kind === 'container-actions') {
    const item = findContainer(sheet.target);
    title = item?.name || sheet.target;
    body = renderContainerActionSheet(sheet.target, item);
  } else if (sheet.kind === 'image-actions') {
    const item = findImage(sheet.target);
    title = item ? imageName(item) : sheet.target;
    body = renderImageActionSheet(sheet.target, item);
  } else if (sheet.kind === 'output') {
    title = sheet.title;
    body = `<pre class="output-pre sheet-output">${escapeHtml(state.output || '暂无输出')}</pre>`;
    footer = `${button('关闭', 'close-sheet', 'ghost', false)}`;
  } else if (sheet.kind === 'confirm') {
    title = sheet.title;
    body = `<p class="confirm-copy">${escapeHtml(sheet.message)}</p>`;
    footer = `${button(sheet.confirmLabel, 'confirm-run', 'danger', Boolean(state.busy))}${button('取消', 'close-sheet', 'ghost', false)}`;
  }
  return `
    <div class="sheet-layer" data-backdrop="sheet">
      <section class="sheet" role="dialog" aria-modal="true" aria-labelledby="sheet-title">
        <div class="sheet-handle"></div>
        <header class="sheet-head">
          <div>
            <p class="eyebrow">Action sheet</p>
            <h2 id="sheet-title">${escapeHtml(title)}</h2>
          </div>
          <button type="button" class="icon-button ghost" data-action="close-sheet" aria-label="关闭">关闭</button>
        </header>
        <div class="sheet-body">${body}</div>
        ${footer ? `<footer class="sheet-footer">${footer}</footer>` : ''}
      </section>
    </div>`;
}

function renderDockerRunForm(): string {
  return `
    <div class="form-grid">
      <label>容器名<input id="name" value="${escapeHtml(state.forms.dockerName)}" placeholder="demo-nginx" autofocus /></label>
      <label>镜像名<input id="image" value="${escapeHtml(state.forms.dockerImage)}" placeholder="nginx:alpine" /></label>
      <label>网络<input id="network" value="${escapeHtml(state.forms.dockerNetwork)}" placeholder="bridge" /></label>
    </div>
    ${accordion(
      'docker-run-advanced',
      '端口、环境变量和挂载',
      'Advanced',
      `<div class="form-grid"><label>端口映射<input id="ports" value="${escapeHtml(state.forms.dockerPorts)}" placeholder="8080:80,8443:443" /></label><label>环境变量<input id="envs" value="${escapeHtml(state.forms.dockerEnv)}" placeholder="KEY=value,DEBUG=1" /></label><label>Bind mount<input id="mounts" value="${escapeHtml(state.forms.dockerMounts)}" placeholder="/sdcard/www:/usr/share/nginx/html" /></label></div>`,
      false,
    )}`;
}

function renderContainerActionSheet(target: string, item?: ContainerData): string {
  const running = item ? isRunningContainer(item) : false;
  return `
    <div class="sheet-summary">
      <span class="badge ${running ? 'green' : 'slate'}">${escapeHtml(item?.status || 'unknown')}</span>
      <small>${escapeHtml(item?.image || target)}</small>
    </div>
    <div class="action-grid">
      <button type="button" data-container-action="start" data-target="${escapeHtml(target)}" ${state.busy || running ? 'disabled' : ''}>启动</button>
      <button type="button" data-container-action="stop" data-target="${escapeHtml(target)}" ${state.busy || !running ? 'disabled' : ''}>停止</button>
      <button type="button" class="ghost" data-container-action="restart" data-target="${escapeHtml(target)}" ${state.busy ? 'disabled' : ''}>重启</button>
      <button type="button" class="ghost" data-container-action="logs" data-target="${escapeHtml(target)}" ${state.busy ? 'disabled' : ''}>日志</button>
      <button type="button" class="ghost" data-container-action="inspect" data-target="${escapeHtml(target)}" ${state.busy ? 'disabled' : ''}>Inspect</button>
      <button type="button" class="danger" data-container-action="delete" data-target="${escapeHtml(target)}" ${state.busy ? 'disabled' : ''}>删除</button>
    </div>`;
}

function renderImageActionSheet(target: string, item?: ImageData): string {
  return `
    <div class="sheet-summary">
      <span class="badge slate">${escapeHtml(item?.size || 'image')}</span>
      <small>${escapeHtml(item?.created || target)}</small>
    </div>
    <div class="action-grid">
      <button type="button" class="danger" data-image-action="remove" data-target="${escapeHtml(target)}" ${state.busy ? 'disabled' : ''}>删除镜像</button>
    </div>`;
}

function findContainer(target: string): ContainerData | undefined {
  return state.containers.find((item) => item.id === target || item.name === target);
}

function findImage(target: string): ImageData | undefined {
  return state.images.find((item) => item.id === target || imageName(item) === target);
}

function captureFocus(): FocusSnapshot | null {
  const element = document.activeElement;
  if (!(element instanceof HTMLInputElement) || !element.id) return null;
  return {
    id: element.id,
    selectionStart: element.selectionStart,
    selectionEnd: element.selectionEnd,
  };
}

function restoreFocus(snapshot: FocusSnapshot | null, focusSheet = false): void {
  queueMicrotask(() => {
    if (snapshot) {
      const element = document.getElementById(snapshot.id);
      if (element instanceof HTMLInputElement) {
        element.focus();
        try {
          if (snapshot.selectionStart !== null && snapshot.selectionEnd !== null) {
            element.setSelectionRange(snapshot.selectionStart, snapshot.selectionEnd);
          }
        } catch {}
        return;
      }
    }
    if (focusSheet && state.ui.sheet.kind !== 'none') {
      const element = app.querySelector<HTMLElement>('.sheet [autofocus], .sheet input, .sheet button');
      element?.focus();
    }
  });
}

function render(snapshot: FocusSnapshot | null = null, focusSheet = false): void {
  app.innerHTML = renderShell();
  restoreFocus(snapshot, focusSheet);
}

function renderPreservingFocus(): void {
  render(captureFocus());
}

function openSheet(sheet: SheetState): void {
  state.ui.sheet = sheet;
  render(null, true);
}

function closeSheet(): void {
  state.ui.sheet = { kind: 'none' };
  render();
}

function openConfirm(title: string, message: string, confirmLabel: string, intent: ConfirmIntent): void {
  openSheet({ kind: 'confirm', title, message, confirmLabel, intent });
}

function activateTab(next: string): boolean {
  if (!isValidTab(next)) return false;
  state.activeTab = next;
  persistActiveTab();
  render();
  return true;
}

function handleInput(event: Event): void {
  const target = event.target;
  if (!(target instanceof HTMLInputElement)) return;
  const value = target.value;
  if (target.id === 'container-search') {
    state.forms.containerSearch = value;
    renderPreservingFocus();
    return;
  }
  if (target.id === 'name') state.forms.dockerName = value;
  if (target.id === 'image') state.forms.dockerImage = value;
  if (target.id === 'network') state.forms.dockerNetwork = value;
  if (target.id === 'ports') state.forms.dockerPorts = value;
  if (target.id === 'envs') state.forms.dockerEnv = value;
  if (target.id === 'mounts') state.forms.dockerMounts = value;
  if (target.id === 'pull-image') state.forms.imageToPull = value;
}

async function handleAction(buttonElement: HTMLElement): Promise<void> {
  const element = buttonElement as HTMLButtonElement;
  if (element.disabled) return;
  const action = buttonElement.dataset.action || '';
  if (action === 'close-sheet') return closeSheet();
  if (action === 'output') return openSheet({ kind: 'output', title: '输出' });
  if (action === 'refresh') return refresh();
  if (action === 'start') return runAction('启动 Docker', 'start-docker');
  if (action === 'stop') return runAction('停止 Docker', 'stop-docker');
  if (action === 'check') return runCheck();
  if (action === 'daemon-logs') return runAction('读取日志', 'daemon-logs', [], false, { showOutput: true });
  if (action === 'add') return addContainer();
  if (action === 'pull-image') return pullImage();
  if (action === 'toggle-autostart') return setAutostart(!state.statusData?.autostart);
  if (action === 'confirm-run') return runConfirmIntent();
}

async function handleContainerAction(buttonElement: HTMLElement): Promise<void> {
  const element = buttonElement as HTMLButtonElement;
  if (element.disabled) return;
  const action = buttonElement.dataset.containerAction || '';
  const target = buttonElement.dataset.target || '';
  if (!target) return;
  if (action === 'more') return openSheet({ kind: 'container-actions', target });
  if (action === 'logs') return runAction('读取容器日志', 'container-logs', [target], false, { showOutput: true });
  if (action === 'inspect') return runAction('Inspect 容器', 'inspect-container', [target], false, { showOutput: true });
  if (action === 'delete') {
    return openConfirm('删除 Docker 容器', `确认删除 ${target}？该操作不可撤销。`, '删除容器', {
      kind: 'delete-container',
      target,
    });
  }
  if (action === 'start') return runAction('启动容器', 'start-container', [target]);
  if (action === 'stop') return runAction('停止容器', 'stop-container', [target]);
  if (action === 'restart') return runAction('重启容器', 'restart-container', [target]);
}

async function handleImageAction(buttonElement: HTMLElement): Promise<void> {
  const element = buttonElement as HTMLButtonElement;
  if (element.disabled) return;
  const action = buttonElement.dataset.imageAction || '';
  const target = buttonElement.dataset.target || '';
  if (!target) return;
  if (action === 'more') return openSheet({ kind: 'image-actions', target });
  if (action === 'remove') return openConfirm('删除镜像', `确认删除镜像 ${target}？`, '删除镜像', { kind: 'remove-image', target });
}

async function runConfirmIntent(): Promise<void> {
  const sheet = state.ui.sheet;
  if (sheet.kind !== 'confirm') return;
  if (sheet.intent.kind === 'delete-container') {
    return runAction('删除容器', 'delete-container', [sheet.intent.target]);
  }
  if (sheet.intent.kind === 'remove-image') {
    return runAction('删除镜像', 'remove-image', [sheet.intent.target]);
  }
}

function handleSheetOpen(buttonElement: HTMLElement): void {
  const sheet = buttonElement.dataset.sheet || '';
  if (sheet === 'docker-run') return openSheet({ kind: 'docker-run' });
  if (sheet === 'image-pull') return openSheet({ kind: 'image-pull' });
}

async function handleClick(event: MouseEvent): Promise<void> {
  const target = event.target;
  if (!(target instanceof HTMLElement)) return;
  if (target.dataset.backdrop === 'sheet') return closeSheet();

  const tab = target.closest<HTMLElement>('[data-tab]');
  if (tab) {
    event.preventDefault();
    activateTab(tab.dataset.tab || '');
    return;
  }

  const accordionButton = target.closest<HTMLElement>('[data-accordion]');
  if (accordionButton) {
    const id = accordionButton.dataset.accordion || '';
    state.ui.expanded[id] = !isExpanded(id);
    persistExpanded();
    render();
    return;
  }

  const sheetButton = target.closest<HTMLElement>('[data-sheet]');
  if (sheetButton) {
    handleSheetOpen(sheetButton);
    return;
  }

  const actionButton = target.closest<HTMLElement>('[data-action]');
  if (actionButton) {
    await handleAction(actionButton);
    return;
  }

  const containerButton = target.closest<HTMLElement>('[data-container-action]');
  if (containerButton) {
    await handleContainerAction(containerButton);
    return;
  }

  const imageButton = target.closest<HTMLElement>('[data-image-action]');
  if (imageButton) {
    await handleImageAction(imageButton);
  }
}

function handleKeydown(event: KeyboardEvent): void {
  if (event.key === 'Escape' && state.ui.sheet.kind !== 'none') {
    event.preventDefault();
    closeSheet();
  }
}

app.addEventListener('click', (event) => {
  void handleClick(event);
});
app.addEventListener('input', handleInput);
document.addEventListener('keydown', handleKeydown);

readConfig();
restorePersistedUi();
render();
void refresh();
