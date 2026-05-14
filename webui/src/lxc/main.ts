import { enableEdgeToEdge, exec, toast } from 'kernelsu';
import '../style.css';

type Tab = 'dashboard' | 'containers' | 'import' | 'diagnostics';

type ExecResult = {
  errno: number;
  stdout: string;
  stderr: string;
};

type ExecOptions = {
  env?: Record<string, string>;
};

type ApiResponse = Record<string, unknown> & {
  ok?: boolean;
  error?: string;
  output?: string;
  containers?: unknown[];
};

type LxcStatus = ApiResponse & {
  base_present?: boolean;
  bridge?: string;
  bridge_subnet?: string;
  containers_running?: number;
  containers_stopped?: number;
  containers_total?: number;
  data_root?: string;
  lxc_containers?: string;
  lxc_runtime?: string;
  module_target?: string;
  runtime?: string;
};

type LxcContainer = {
  name: string;
  state: string;
  pid: string;
  distro: string;
  release: string;
  arch: string;
  rootfs: string;
  config: string;
  log: string;
  autostart: boolean;
};

type NavItem = {
  tab: Tab;
  label: string;
};

const DEFAULT_API = '/data/adb/modules/achost-lxc/achost/bin/achost-webui-api.sh';
const app = document.querySelector<HTMLDivElement>('#app');

const state = {
  apiPath: DEFAULT_API,
  activeTab: 'dashboard' as Tab,
  busy: '',
  status: null as LxcStatus | null,
  containers: [] as LxcContainer[],
  output: '',
  ui: {
    expanded: {} as Record<string, boolean>,
  },
  forms: {
    importName: 'ubuntu-26.04',
    rootfsPath: '',
    rootfsSha256: '',
    distro: 'ubuntu',
    release: '26.04',
    arch: 'arm64',
    startAfter: false,
    execTarget: '',
    execCommand: 'cat /etc/os-release || uname -a',
    passwordTarget: 'ubuntu-26.04',
    passwordUser: 'root',
    passwordValue: '',
  },
};

try {
  enableEdgeToEdge(true);
} catch {}

function readConfig(): void {
  const meta = document.querySelector<HTMLMetaElement>('meta[name="achost-webui-config"]');
  const raw = meta ? meta.content : '';
  if (!raw) return;
  try {
    const config = JSON.parse(raw) as { api?: unknown };
    if (typeof config.api === 'string' && config.api.startsWith('/data/adb/modules/')) {
      state.apiPath = config.api;
    }
  } catch {}
}

function escapeHtml(value: unknown): string {
  return String(value == null ? '' : value)
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

function nextFrame(): Promise<void> {
  return new Promise((resolve) => requestAnimationFrame(() => resolve()));
}

function validName(value: string): boolean {
  return /^[A-Za-z0-9_.-]+$/.test(value) && value !== '.' && value !== '..' && value.indexOf('..') === -1;
}

function validLabel(value: string): boolean {
  return /^[A-Za-z0-9_.-]+$/.test(value);
}

function validAndroidPath(value: string): boolean {
  return value.startsWith('/') && !/[\x00-\x1F\x7F]/.test(value);
}

function validSha256(value: string): boolean {
  return /^[A-Fa-f0-9]{64}$/.test(value);
}

function validLinuxUser(value: string): boolean {
  return /^[A-Za-z_][A-Za-z0-9_.-]{0,63}$/.test(value);
}

function validPassword(value: string): boolean {
  return value.length > 0 && !/[\x00-\x1F\x7F:\r\n]/.test(value);
}

async function callApi(action: string, args: string[] = []): Promise<ApiResponse> {
  const command = [state.apiPath, action].concat(args).map(shellQuote).join(' ');
  const result = (await exec(command)) as ExecResult;
  return parseApiResult(result);
}

async function callApiWithEnv(action: string, args: string[], env: Record<string, string>): Promise<ApiResponse> {
  const command = [state.apiPath, action].concat(args).map(shellQuote).join(' ');
  const execWithOptions = exec as unknown as (command: string, options?: ExecOptions) => Promise<ExecResult>;
  const result = await execWithOptions(command, { env });
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

function parseContainers(value: unknown): LxcContainer[] {
  if (!Array.isArray(value)) return [];
  return value
    .map((item) => {
      const row = item as Record<string, unknown>;
      return {
        name: String(row.name || ''),
        state: String(row.state || 'UNKNOWN'),
        pid: typeof row.pid === 'string' ? row.pid : '',
        distro: String(row.distro || 'unknown'),
        release: String(row.release || 'unknown'),
        arch: String(row.arch || 'unknown'),
        rootfs: String(row.rootfs || ''),
        config: String(row.config || ''),
        log: String(row.log || ''),
        autostart: Boolean(row.autostart),
      };
    })
    .filter((item) => item.name);
}

function isRunning(container: LxcContainer): boolean {
  return container.state.toLowerCase() === 'running';
}

function numberMetric(value: unknown, fallback: number | string): number | string {
  if (typeof value === 'number') return value;
  if (typeof value === 'string' && value) {
    const parsed = Number(value);
    if (Number.isFinite(parsed)) return parsed;
  }
  return fallback;
}

async function refresh(): Promise<void> {
  state.busy = 'refresh';
  render();
  try {
    const status = (await callApi('lxc-status')) as LxcStatus;
    const list = await callApi('lxc-list');
    state.status = status;
    state.containers = parseContainers(list.containers || status.containers);
    if (status.ok === false) state.output = responseText(status);
    if (list.ok === false) state.output = responseText(list);
  } catch (error) {
    state.output = error instanceof Error ? error.message : String(error);
    notify('刷新失败');
  } finally {
    state.busy = '';
    render();
  }
}

async function runAction(label: string, action: string, args: string[] = [], refreshAfter = true): Promise<void> {
  state.busy = action;
  render();
  await nextFrame();
  try {
    const response = await callApi(action, args);
    state.output = responseText(response);
    notify(response.ok === false ? `${label}失败` : `${label}完成`);
    if (refreshAfter) await refresh();
  } catch (error) {
    state.output = error instanceof Error ? error.message : String(error);
    notify(`${label}失败`);
  } finally {
    state.busy = '';
    render();
  }
}

async function importRootfs(): Promise<void> {
  const name = state.forms.importName.trim();
  const rootfs = state.forms.rootfsPath.trim();
  const sha256 = state.forms.rootfsSha256.trim();
  const distro = state.forms.distro.trim() || 'unknown';
  const release = state.forms.release.trim() || 'unknown';
  const arch = state.forms.arch.trim() || 'unknown';
  if (!validName(name)) return notify('LXC 容器名不合法');
  if (!validAndroidPath(rootfs)) return notify('rootfs 路径必须是 Android 绝对路径');
  if (sha256 && !validSha256(sha256)) return notify('SHA-256 必须是 64 位十六进制');
  if (![distro, release, arch].every(validLabel)) return notify('rootfs 元数据只能包含字母、数字、点、下划线和短横线');

  state.busy = 'lxc-import-rootfs';
  render();
  const outputs: string[] = [];
  let failed = false;
  try {
    const args = [name, rootfs, distro, release, arch];
    if (sha256) args.push(sha256.toLowerCase());
    const imported = await callApi('lxc-import-rootfs', args);
    failed = imported.ok === false;
    outputs.push(`## 导入 rootfs\n${responseText(imported)}`);
    if (!failed && state.forms.startAfter) {
      const started = await callApi('lxc-start', [name]);
      failed = started.ok === false;
      outputs.push(`## 启动容器\n${responseText(started)}`);
    }
    state.output = outputs.join('\n\n');
    notify(failed ? '导入 rootfs 失败' : '导入 rootfs 完成');
    await refresh();
  } catch (error) {
    state.output = outputs.concat(error instanceof Error ? error.message : String(error)).join('\n\n');
    notify('导入 rootfs 失败');
  } finally {
    state.busy = '';
    render();
  }
}

async function setPassword(): Promise<void> {
  const name = state.forms.passwordTarget.trim();
  const user = state.forms.passwordUser.trim();
  const password = state.forms.passwordValue;
  if (!validName(name)) return notify('LXC 容器名不合法');
  if (!validLinuxUser(user)) return notify('Linux 用户名不合法');
  if (!validPassword(password)) return notify('密码不能为空，且不能包含冒号、换行或控制字符');
  state.busy = 'lxc-set-password';
  render();
  try {
    const response = await callApiWithEnv('lxc-set-password', [name, user], { ACHOST_LXC_PASSWORD: password });
    state.output = responseText(response);
    state.forms.passwordValue = '';
    notify(response.ok === false ? '设置密码失败' : '设置密码完成');
  } catch (error) {
    state.output = error instanceof Error ? error.message : String(error);
    state.forms.passwordValue = '';
    notify('设置密码失败');
  } finally {
    state.busy = '';
    render();
  }
}

function button(label: string, action: string, className = '', disabled = false): string {
  return `<button type="button" class="${escapeHtml(className)}" data-action="${escapeHtml(action)}" ${disabled ? 'disabled' : ''}>${escapeHtml(label)}</button>`;
}

function availableTabs(): NavItem[] {
  return [
    { tab: 'dashboard', label: '概览' },
    { tab: 'containers', label: '容器' },
    { tab: 'import', label: '导入' },
    { tab: 'diagnostics', label: '诊断' },
  ];
}

function nav(className: string): string {
  return `<nav class="${escapeHtml(className)}" aria-label="主导航">${availableTabs()
    .map(
      (item) => `<button type="button" class="nav-item ${state.activeTab === item.tab ? 'active' : ''}" data-tab="${item.tab}" ${state.activeTab === item.tab ? 'aria-current="page"' : ''}><span>${escapeHtml(item.label)}</span></button>`,
    )
    .join('')}</nav>`;
}

function statusPill(): string {
  if (!state.status) return '<span class="pill stop"><span></span>LXC 未知</span>';
  if (state.status.ok === false) return '<span class="pill stop"><span></span>LXC 异常</span>';
  const running = numberMetric(state.status.containers_running, state.containers.filter(isRunning).length);
  return `<span class="pill ok"><span></span>LXC · ${escapeHtml(running)} 运行</span>`;
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

function detailGrid(rows: Array<[string, unknown]>): string {
  return `<div class="detail-grid">${rows.map(([label, value]) => detailItem(label, value)).join('')}</div>`;
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

function renderDashboard(): string {
  const status = state.status;
  const autostartCount = state.containers.filter((item) => item.autostart).length;
  return `
    ${metricStrip([
      ['容器', numberMetric(status?.containers_total, state.containers.length || '—')],
      ['运行', numberMetric(status?.containers_running, state.containers.filter(isRunning).length), 'success'],
      ['停止', numberMetric(status?.containers_stopped, '—'), 'warning'],
      ['自启', autostartCount || '—'],
    ])}
    ${actionPanel(
      'LXC 控制台',
      `${button('刷新', 'refresh', 'ghost', Boolean(state.busy))}${button('运行检查', 'check', '', Boolean(state.busy))}${button('导入 rootfs', 'open-import', 'ghost', Boolean(state.busy))}`,
    )}
    ${accordion(
      'lxc-runtime',
      '基础模块详情',
      'LXC Runtime',
      detailGrid([
        ['LXC Runtime', status?.lxc_runtime],
        ['容器目录', status?.lxc_containers],
        ['Bridge', status?.bridge || 'lxcbr0'],
        ['Bridge subnet', status?.bridge_subnet],
        ['Base 模块', status?.base_present ? 'present' : 'missing'],
        ['Module target', status?.module_target || 'lxc'],
        ['Data root', status?.data_root],
        ['API', state.apiPath],
      ]),
      false,
      statusPill(),
    )}
    ${renderOutputAccordion('lxc-output')}
  `;
}

function renderContainerRows(): string {
  if (!state.containers.length) return '<div class="empty">暂无 LXC 容器。先在导入页导入 rootfs。</div>';
  return state.containers
    .map((item) => {
      const running = isRunning(item);
      const primaryAction = running ? 'stop' : 'start';
      const autostartAction = item.autostart ? 'autostart-off' : 'autostart-on';
      return `<article class="entity-card lxc-container-card">
        <div class="lxc-card-head">
          <div class="lxc-card-title">
            <strong title="${escapeHtml(item.name)}">${escapeHtml(item.name)}</strong>
            <small>${escapeHtml(`${item.distro} ${item.release} / ${item.arch}`)}</small>
          </div>
          <div class="lxc-card-badges">
            <span class="badge ${running ? 'green' : 'slate'}">${escapeHtml(item.state)}${item.pid ? ` · pid ${escapeHtml(item.pid)}` : ''}</span>
            <span class="badge ${item.autostart ? 'green' : 'slate'}">自启 ${item.autostart ? 'on' : 'off'}</span>
          </div>
        </div>
        <div class="lxc-card-details">
          <div class="lxc-card-detail"><b>rootfs</b><span title="${escapeHtml(item.rootfs)}">${escapeHtml(item.rootfs || '—')}</span></div>
          <div class="lxc-card-detail"><b>config</b><span title="${escapeHtml(item.config)}">${escapeHtml(item.config || '—')}</span></div>
        </div>
        <div class="lxc-card-actions">
          <button type="button" class="small" data-container="${escapeHtml(item.name)}" data-container-action="${primaryAction}" ${state.busy ? 'disabled' : ''}>${running ? '停止' : '启动'}</button>
          ${running ? `<button type="button" class="small danger" data-container="${escapeHtml(item.name)}" data-container-action="force-stop" ${state.busy ? 'disabled' : ''}>强制停止</button>` : ''}
          <button type="button" class="small ghost" data-container="${escapeHtml(item.name)}" data-container-action="${autostartAction}" ${state.busy ? 'disabled' : ''}>自启${item.autostart ? '关' : '开'}</button>
          <button type="button" class="small ghost" data-container="${escapeHtml(item.name)}" data-container-action="status" ${state.busy ? 'disabled' : ''}>状态</button>
          <button type="button" class="small ghost" data-container="${escapeHtml(item.name)}" data-container-action="logs" ${state.busy ? 'disabled' : ''}>日志</button>
          <button type="button" class="small danger" data-container="${escapeHtml(item.name)}" data-container-action="destroy" ${state.busy ? 'disabled' : ''}>删除</button>
        </div>
      </article>`;
    })
    .join('');
}

function renderContainers(): string {
  return `<section class="panel list-tools"><div><p class="eyebrow">Inventory</p><h2>LXC 容器</h2></div>${button('刷新', 'refresh', 'ghost', Boolean(state.busy))}</section><section class="entity-list">${renderContainerRows()}</section>${renderOutputAccordion('lxc-container-output')}`;
}

function renderImport(): string {
  return `<section class="panel form-panel">
    <div><p class="eyebrow">Import rootfs</p><h2>导入 LXC rootfs</h2></div>
    <p class="form-note">先通过 adb 或文件管理器把 rootfs tar/tar.gz 放到设备绝对路径，再在这里导入。SHA-256 填写时会先校验。</p>
    <div class="form-grid primary-form">
      <label>容器名<input id="import-name" value="${escapeHtml(state.forms.importName)}" placeholder="ubuntu-26.04" /></label>
      <label>rootfs 路径<input id="rootfs-path" value="${escapeHtml(state.forms.rootfsPath)}" placeholder="/data/local/tmp/ubuntu-26.04-arm64-rootfs.tar.gz" /></label>
    </div>
    <div class="form-grid">
      <label>rootfs SHA-256<input id="rootfs-sha256" value="${escapeHtml(state.forms.rootfsSha256)}" placeholder="可选" /></label>
      <label>Distro<input id="distro" value="${escapeHtml(state.forms.distro)}" /></label>
      <label>Release<input id="release" value="${escapeHtml(state.forms.release)}" /></label>
      <label>Arch<input id="arch" value="${escapeHtml(state.forms.arch)}" /></label>
    </div>
    <div class="option-stack">
      <label class="check-row"><input id="start-after" type="checkbox" ${state.forms.startAfter ? 'checked' : ''} /><span>导入成功后启动容器</span></label>
    </div>
    <div class="button-row">${button('导入 rootfs', 'import-rootfs', '', Boolean(state.busy))}</div>
  </section>${renderOutputAccordion('lxc-import-output')}`;
}

function renderDiagnostics(): string {
  return `${actionPanel(
    '诊断与维护',
    `${button('运行 LXC 检查', 'check', '', Boolean(state.busy))}${button('刷新状态', 'refresh', 'ghost', Boolean(state.busy))}`,
  )}
  <section class="panel form-panel"><div><p class="eyebrow">Exec</p><h2>容器命令</h2></div><div class="form-grid"><label>容器名<input id="exec-target" value="${escapeHtml(state.forms.execTarget)}" /></label><label>命令<input id="exec-command" value="${escapeHtml(state.forms.execCommand)}" /></label></div><div class="button-row">${button('执行', 'exec', '', Boolean(state.busy))}</div></section>
  <section class="panel form-panel"><div><p class="eyebrow">Password</p><h2>用户密码</h2></div><p class="form-note warning-note">直接更新容器 rootfs 的 /etc/shadow；只用于可信本机管理场景。</p><div class="form-grid"><label>容器名<input id="password-target" value="${escapeHtml(state.forms.passwordTarget)}" /></label><label>Linux 用户<input id="password-user" value="${escapeHtml(state.forms.passwordUser)}" /></label><label>密码<input id="password-value" type="password" value="${escapeHtml(state.forms.passwordValue)}" /></label></div><div class="button-row">${button('生成密码', 'generate-password', 'ghost', Boolean(state.busy))}${button('设置密码', 'set-password', '', Boolean(state.busy))}</div></section>
  ${renderOutputAccordion('lxc-diagnostics-output', state.status?.error)}`;
}

function renderOutputAccordion(id: string, fallback?: unknown): string {
  const text = state.output || fallback || '';
  if (!text) return '';
  return accordion(
    id,
    '最近输出',
    'Output',
    `<pre class="output-pre">${escapeHtml(text)}</pre>`,
    false,
  );
}

function renderMain(): string {
  if (state.activeTab === 'containers') return renderContainers();
  if (state.activeTab === 'import') return renderImport();
  if (state.activeTab === 'diagnostics') return renderDiagnostics();
  return renderDashboard();
}

function renderTopActions(): string {
  return `${statusPill()}${button(state.busy === 'refresh' ? '刷新中…' : '刷新', 'refresh', 'ghost', Boolean(state.busy))}${button('检查', 'check', '', Boolean(state.busy))}`;
}

function renderBrand(): string {
  return `<div class="brand"><span class="logo">AC</span><div><strong>ACHost</strong><small>LXC Panel</small></div></div>`;
}

function render(): void {
  if (!app) return;
  app.innerHTML = `<main class="app-shell lxc-shell">
    <aside class="side-rail">${renderBrand()}${nav('side-nav')}</aside>
    <section class="workspace">
      <header class="mobile-header">${renderBrand()}${statusPill()}</header>
      ${pageIntro('LXC 容器面板')}
      <div class="page-stack">${renderMain()}</div>
    </section>
    ${nav('bottom-nav')}
  </main>`;
}

function handleInput(event: Event): void {
  const target = event.target;
  if (!(target instanceof HTMLInputElement)) return;
  const value = target.value;
  if (target.id === 'import-name') state.forms.importName = value;
  if (target.id === 'rootfs-path') state.forms.rootfsPath = value;
  if (target.id === 'rootfs-sha256') state.forms.rootfsSha256 = value;
  if (target.id === 'distro') state.forms.distro = value;
  if (target.id === 'release') state.forms.release = value;
  if (target.id === 'arch') state.forms.arch = value;
  if (target.id === 'start-after') state.forms.startAfter = target.checked;
  if (target.id === 'exec-target') state.forms.execTarget = value;
  if (target.id === 'exec-command') state.forms.execCommand = value;
  if (target.id === 'password-target') state.forms.passwordTarget = value;
  if (target.id === 'password-user') state.forms.passwordUser = value;
  if (target.id === 'password-value') state.forms.passwordValue = value;
}

async function handleAction(action: string): Promise<void> {
  if (action === 'refresh') return refresh();
  if (action === 'check') return runAction('运行 LXC 检查', 'lxc-check', [], false);
  if (action === 'open-import') {
    state.activeTab = 'import';
    render();
    return;
  }
  if (action === 'import-rootfs') return importRootfs();
  if (action === 'exec') {
    const name = state.forms.execTarget.trim();
    const command = state.forms.execCommand.trim();
    if (!validName(name)) return notify('LXC 容器名不合法');
    if (!command) return notify('请输入命令');
    return runAction('执行容器命令', 'lxc-exec', [name, command], false);
  }
  if (action === 'generate-password') {
    const name = state.forms.passwordTarget.trim();
    const user = state.forms.passwordUser.trim();
    if (!validName(name)) return notify('LXC 容器名不合法');
    if (!validLinuxUser(user)) return notify('Linux 用户名不合法');
    return runAction('生成密码', 'lxc-generate-password', [name, user], false);
  }
  if (action === 'set-password') return setPassword();
}

async function handleContainerAction(target: HTMLElement): Promise<void> {
  const name = target.dataset.container || '';
  const action = target.dataset.containerAction || '';
  if (!validName(name)) return notify('LXC 容器名不合法');
  if (action === 'start') return runAction('启动容器', 'lxc-start', [name]);
  if (action === 'stop') return runAction('停止容器', 'lxc-stop', [name]);
  if (action === 'force-stop') return runAction('强制停止容器', 'lxc-force-stop', [name]);
  if (action === 'autostart-on') return runAction('开启容器自启', 'lxc-set-autostart', [name, 'on']);
  if (action === 'autostart-off') return runAction('关闭容器自启', 'lxc-set-autostart', [name, 'off']);
  if (action === 'status') return runAction('读取容器状态', 'lxc-system-status', [name], false);
  if (action === 'logs') return runAction('读取容器日志', 'lxc-logs', [name], false);
  if (action === 'destroy') {
    if (!window.confirm(`删除 LXC 容器 ${name}？容器 rootfs 和配置都会被删除。`)) return;
    return runAction('删除容器', 'lxc-destroy', [name]);
  }
}

if (app) {
  app.addEventListener('click', (event) => {
    const target = event.target;
    if (!(target instanceof HTMLElement)) return;
    const tab = target.closest<HTMLElement>('[data-tab]');
    if (tab) {
      state.activeTab = (tab.dataset.tab || 'dashboard') as Tab;
      render();
      return;
    }
    const accordionButton = target.closest<HTMLElement>('[data-accordion]');
    if (accordionButton) {
      const id = accordionButton.dataset.accordion || '';
      state.ui.expanded[id] = !isExpanded(id);
      render();
      return;
    }
    const containerAction = target.closest<HTMLElement>('[data-container-action]');
    if (containerAction) {
      void handleContainerAction(containerAction);
      return;
    }
    const action = target.closest<HTMLElement>('[data-action]');
    if (action) void handleAction(action.dataset.action || '');
  });

  app.addEventListener('input', handleInput);
}
readConfig();
render();
void refresh();
