(function(){const r=document.createElement("link").relList;if(r&&r.supports&&r.supports("modulepreload"))return;for(const a of document.querySelectorAll('link[rel="modulepreload"]'))o(a);new MutationObserver(a=>{for(const i of a)if(i.type==="childList")for(const c of i.addedNodes)c.tagName==="LINK"&&c.rel==="modulepreload"&&o(c)}).observe(document,{childList:!0,subtree:!0});function n(a){const i={};return a.integrity&&(i.integrity=a.integrity),a.referrerPolicy&&(i.referrerPolicy=a.referrerPolicy),a.crossOrigin==="use-credentials"?i.credentials="include":a.crossOrigin==="anonymous"?i.credentials="omit":i.credentials="same-origin",i}function o(a){if(a.ep)return;a.ep=!0;const i=n(a);fetch(a.href,i)}})();let E=0;function F(t){return`${t}_callback_${Date.now()}_${E++}`}function T(t,r){return typeof r>"u"&&(r={}),new Promise((n,o)=>{const a=F("exec");window[a]=(c,m,p)=>{n({errno:c,stdout:m,stderr:p}),i(a)};function i(c){delete window[c]}try{ksu.exec(t,JSON.stringify(r),a)}catch(c){o(c),i(a)}})}function I(t){ksu.enableEdgeToEdge(t)}function U(t){ksu.toast(t)}const H="/data/adb/modules/achost-lxc/achost/bin/achost-webui-api.sh",g=document.querySelector("#app"),e={apiPath:H,activeTab:"dashboard",busy:"",status:null,containers:[],output:"",ui:{expanded:{}},forms:{importName:"ubuntu-26.04",rootfsPath:"",rootfsSha256:"",distro:"ubuntu",release:"26.04",arch:"arm64",startAfter:!1,execTarget:"",execCommand:"cat /etc/os-release || uname -a",passwordTarget:"ubuntu-26.04",passwordUser:"root",passwordValue:""}};try{I(!0)}catch{}function j(){const t=document.querySelector('meta[name="achost-webui-config"]'),r=t?t.content:"";if(r)try{const n=JSON.parse(r);typeof n.api=="string"&&n.api.startsWith("/data/adb/modules/")&&(e.apiPath=n.api)}catch{}}function s(t){return String(t??"").replace(/&/g,"&amp;").replace(/</g,"&lt;").replace(/>/g,"&gt;").replace(/"/g,"&quot;").replace(/'/g,"&#39;")}function P(t){return`'${t.replace(/'/g,"'\\''")}'`}function u(t){try{U(t)}catch{}}function M(){return new Promise(t=>requestAnimationFrame(()=>t()))}function h(t){return/^[A-Za-z0-9_.-]+$/.test(t)&&t!=="."&&t!==".."&&t.indexOf("..")===-1}function R(t){return/^[A-Za-z0-9_.-]+$/.test(t)}function W(t){return t.startsWith("/")&&!/[\x00-\x1F\x7F]/.test(t)}function D(t){return/^[A-Fa-f0-9]{64}$/.test(t)}function X(t){return/^[A-Za-z_][A-Za-z0-9_.-]{0,63}$/.test(t)}function q(t){return t.length>0&&!/[\x00-\x1F\x7F:\r\n]/.test(t)}async function $(t,r=[]){const n=[e.apiPath,t].concat(r).map(P).join(" "),o=await T(n);return _(o)}async function z(t,r,n){const o=[e.apiPath,t].concat(r).map(P).join(" "),i=await T(o,{env:n});return _(i)}function _(t){const r=t.stdout.trim()||t.stderr.trim();if(!r)return{ok:!1,error:`命令没有输出，errno=${t.errno}`};try{const n=JSON.parse(r);return t.errno!==0&&n.ok!==!1&&(n.ok=!1,n.error=`errno=${t.errno}`),n}catch{return{ok:!1,error:r}}}function b(t){return typeof t.output=="string"?t.output:typeof t.error=="string"?t.error:JSON.stringify(t,null,2)}function V(t){return Array.isArray(t)?t.map(r=>{const n=r;return{name:String(n.name||""),state:String(n.state||"UNKNOWN"),pid:typeof n.pid=="string"?n.pid:"",distro:String(n.distro||"unknown"),release:String(n.release||"unknown"),arch:String(n.arch||"unknown"),rootfs:String(n.rootfs||""),config:String(n.config||""),log:String(n.log||""),autostart:!!n.autostart}}).filter(r=>r.name):[]}function x(t){return t.state.toLowerCase()==="running"}function y(t,r){if(typeof t=="number")return t;if(typeof t=="string"&&t){const n=Number(t);if(Number.isFinite(n))return n}return r}async function v(){e.busy="refresh",d();try{const t=await $("lxc-status"),r=await $("lxc-list");e.status=t,e.containers=V(r.containers||t.containers),t.ok===!1&&(e.output=b(t)),r.ok===!1&&(e.output=b(r))}catch(t){e.output=t instanceof Error?t.message:String(t),u("刷新失败")}finally{e.busy="",d()}}async function f(t,r,n=[],o=!0){e.busy=r,d(),await M();try{const a=await $(r,n);e.output=b(a),u(a.ok===!1?`${t}失败`:`${t}完成`),o&&await v()}catch(a){e.output=a instanceof Error?a.message:String(a),u(`${t}失败`)}finally{e.busy="",d()}}async function J(){const t=e.forms.importName.trim(),r=e.forms.rootfsPath.trim(),n=e.forms.rootfsSha256.trim(),o=e.forms.distro.trim()||"unknown",a=e.forms.release.trim()||"unknown",i=e.forms.arch.trim()||"unknown";if(!h(t))return u("LXC 容器名不合法");if(!W(r))return u("rootfs 路径必须是 Android 绝对路径");if(n&&!D(n))return u("SHA-256 必须是 64 位十六进制");if(![o,a,i].every(R))return u("rootfs 元数据只能包含字母、数字、点、下划线和短横线");e.busy="lxc-import-rootfs",d();const c=[];let m=!1;try{const p=[t,r,o,a,i];n&&p.push(n.toLowerCase());const A=await $("lxc-import-rootfs",p);if(m=A.ok===!1,c.push(`## 导入 rootfs
${b(A)}`),!m&&e.forms.startAfter){const L=await $("lxc-start",[t]);m=L.ok===!1,c.push(`## 启动容器
${b(L)}`)}e.output=c.join(`

`),u(m?"导入 rootfs 失败":"导入 rootfs 完成"),await v()}catch(p){e.output=c.concat(p instanceof Error?p.message:String(p)).join(`

`),u("导入 rootfs 失败")}finally{e.busy="",d()}}async function Z(){const t=e.forms.passwordTarget.trim(),r=e.forms.passwordUser.trim(),n=e.forms.passwordValue;if(!h(t))return u("LXC 容器名不合法");if(!X(r))return u("Linux 用户名不合法");if(!q(n))return u("密码不能为空，且不能包含冒号、换行或控制字符");e.busy="lxc-set-password",d();try{const o=await z("lxc-set-password",[t,r],{ACHOST_LXC_PASSWORD:n});e.output=b(o),e.forms.passwordValue="",u(o.ok===!1?"设置密码失败":"设置密码完成")}catch(o){e.output=o instanceof Error?o.message:String(o),e.forms.passwordValue="",u("设置密码失败")}finally{e.busy="",d()}}function l(t,r,n="",o=!1){return`<button type="button" class="${s(n)}" data-action="${s(r)}" ${o?"disabled":""}>${s(t)}</button>`}function K(){return[{tab:"dashboard",label:"概览"},{tab:"containers",label:"容器"},{tab:"import",label:"导入"},{tab:"diagnostics",label:"诊断"}]}function k(t){return`<nav class="${s(t)}" aria-label="主导航">${K().map(r=>`<button type="button" class="nav-item ${e.activeTab===r.tab?"active":""}" data-tab="${r.tab}" ${e.activeTab===r.tab?'aria-current="page"':""}><span>${s(r.label)}</span></button>`).join("")}</nav>`}function C(){if(!e.status)return'<span class="pill stop"><span></span>LXC 未知</span>';if(e.status.ok===!1)return'<span class="pill stop"><span></span>LXC 异常</span>';const t=y(e.status.containers_running,e.containers.filter(x).length);return`<span class="pill ok"><span></span>LXC · ${s(t)} 运行</span>`}function G(t,r,n=""){return`<article class="metric-card ${n}"><span>${s(t)}</span><strong>${s(r)}</strong></article>`}function Q(t){return`<section class="metric-strip">${t.map(([r,n,o])=>G(r,n,o)).join("")}</section>`}function Y(t,r){return`<div class="detail-item"><span>${s(t)}</span><strong>${s(r||"—")}</strong></div>`}function tt(t){return`<div class="detail-grid">${t.map(([r,n])=>Y(r,n)).join("")}</div>`}function N(t,r=!1){return e.ui.expanded[t]??r}function B(t,r,n,o,a=!1,i=""){const c=N(t,a);return`
    <section class="panel accordion ${c?"open":""}">
      <button type="button" class="accordion-trigger" data-accordion="${s(t)}" aria-expanded="${c?"true":"false"}">
        <span>
          <small>${s(n)}</small>
          <strong>${s(r)}</strong>
        </span>
        <span class="accordion-side">${i}<span class="chevron">${c?"收起":"展开"}</span></span>
      </button>
      ${c?`<div class="accordion-body">${o}</div>`:""}
    </section>`}function rt(t){return`
    <section class="page-intro">
      <div>
        <p class="eyebrow">KernelSU Module WebUI</p>
        <h1>${s(t)}</h1>
      </div>
      <div class="top-actions">${ct()}</div>
    </section>`}function O(t,r){return`
    <section class="panel command-panel">
      <div>
        <p class="eyebrow">Control</p>
        <h2>${s(t)}</h2>
      </div>
      <div class="button-row">${r}</div>
    </section>`}function et(){const t=e.status,r=e.containers.filter(n=>n.autostart).length;return`
    ${Q([["容器",y(t?.containers_total,e.containers.length||"—")],["运行",y(t?.containers_running,e.containers.filter(x).length),"success"],["停止",y(t?.containers_stopped,"—"),"warning"],["自启",r||"—"]])}
    ${O("LXC 控制台",`${l("刷新","refresh","ghost",!!e.busy)}${l("运行检查","check","",!!e.busy)}${l("导入 rootfs","open-import","ghost",!!e.busy)}`)}
    ${B("lxc-runtime","基础模块详情","LXC Runtime",tt([["LXC Runtime",t?.lxc_runtime],["容器目录",t?.lxc_containers],["Bridge",t?.bridge||"lxcbr0"],["Bridge subnet",t?.bridge_subnet],["Base 模块",t?.base_present?"present":"missing"],["Module target",t?.module_target||"lxc"],["Data root",t?.data_root],["API",e.apiPath]]),!1,C())}
    ${w("lxc-output")}
  `}function nt(){return e.containers.length?e.containers.map(t=>{const r=x(t),n=r?"stop":"start",o=t.autostart?"autostart-off":"autostart-on";return`<article class="entity-card lxc-container-card">
        <div class="entity-main">
          <strong title="${s(t.name)}">${s(t.name)}</strong>
          <small>${s(`${t.distro} ${t.release} / ${t.arch}`)}</small>
        </div>
        <div class="entity-meta">
          <span title="${s(t.rootfs)}">rootfs: ${s(t.rootfs||"—")}</span>
          <small title="${s(t.config)}">config: ${s(t.config||"—")}</small>
        </div>
        <div class="entity-status">
          <span class="badge ${r?"green":"slate"}">${s(t.state)}${t.pid?` · pid ${s(t.pid)}`:""}</span>
          <span class="badge ${t.autostart?"green":"slate"}">自启 ${t.autostart?"on":"off"}</span>
        </div>
        <div class="entity-actions">
          <button type="button" class="small" data-container="${s(t.name)}" data-container-action="${n}" ${e.busy?"disabled":""}>${r?"停止":"启动"}</button>
          ${r?`<button type="button" class="small danger" data-container="${s(t.name)}" data-container-action="force-stop" ${e.busy?"disabled":""}>强制停止</button>`:""}
          <button type="button" class="small ghost" data-container="${s(t.name)}" data-container-action="${o}" ${e.busy?"disabled":""}>自启${t.autostart?"关":"开"}</button>
          <button type="button" class="small ghost" data-container="${s(t.name)}" data-container-action="status" ${e.busy?"disabled":""}>状态</button>
          <button type="button" class="small ghost" data-container="${s(t.name)}" data-container-action="logs" ${e.busy?"disabled":""}>日志</button>
          <button type="button" class="small danger" data-container="${s(t.name)}" data-container-action="destroy" ${e.busy?"disabled":""}>删除</button>
        </div>
      </article>`}).join(""):'<div class="empty">暂无 LXC 容器。先在导入页导入 rootfs。</div>'}function st(){return`<section class="panel list-tools"><div><p class="eyebrow">Inventory</p><h2>LXC 容器</h2></div>${l("刷新","refresh","ghost",!!e.busy)}</section><section class="entity-list">${nt()}</section>${w("lxc-container-output")}`}function at(){return`<section class="panel form-panel">
    <div><p class="eyebrow">Import rootfs</p><h2>导入 LXC rootfs</h2></div>
    <p class="form-note">先通过 adb 或文件管理器把 rootfs tar/tar.gz 放到设备绝对路径，再在这里导入。SHA-256 填写时会先校验。</p>
    <div class="form-grid primary-form">
      <label>容器名<input id="import-name" value="${s(e.forms.importName)}" placeholder="ubuntu-26.04" /></label>
      <label>rootfs 路径<input id="rootfs-path" value="${s(e.forms.rootfsPath)}" placeholder="/data/local/tmp/ubuntu-26.04-arm64-rootfs.tar.gz" /></label>
    </div>
    <div class="form-grid">
      <label>rootfs SHA-256<input id="rootfs-sha256" value="${s(e.forms.rootfsSha256)}" placeholder="可选" /></label>
      <label>Distro<input id="distro" value="${s(e.forms.distro)}" /></label>
      <label>Release<input id="release" value="${s(e.forms.release)}" /></label>
      <label>Arch<input id="arch" value="${s(e.forms.arch)}" /></label>
    </div>
    <div class="option-stack">
      <label class="check-row"><input id="start-after" type="checkbox" ${e.forms.startAfter?"checked":""} /><span>导入成功后启动容器</span></label>
    </div>
    <div class="button-row">${l("导入 rootfs","import-rootfs","",!!e.busy)}</div>
  </section>${w("lxc-import-output")}`}function ot(){return`${O("诊断与维护",`${l("运行 LXC 检查","check","",!!e.busy)}${l("刷新状态","refresh","ghost",!!e.busy)}`)}
  <section class="panel form-panel"><div><p class="eyebrow">Exec</p><h2>容器命令</h2></div><div class="form-grid"><label>容器名<input id="exec-target" value="${s(e.forms.execTarget)}" /></label><label>命令<input id="exec-command" value="${s(e.forms.execCommand)}" /></label></div><div class="button-row">${l("执行","exec","",!!e.busy)}</div></section>
  <section class="panel form-panel"><div><p class="eyebrow">Password</p><h2>用户密码</h2></div><p class="form-note warning-note">直接更新容器 rootfs 的 /etc/shadow；只用于可信本机管理场景。</p><div class="form-grid"><label>容器名<input id="password-target" value="${s(e.forms.passwordTarget)}" /></label><label>Linux 用户<input id="password-user" value="${s(e.forms.passwordUser)}" /></label><label>密码<input id="password-value" type="password" value="${s(e.forms.passwordValue)}" /></label></div><div class="button-row">${l("生成密码","generate-password","ghost",!!e.busy)}${l("设置密码","set-password","",!!e.busy)}</div></section>
  ${w("lxc-diagnostics-output",e.status?.error)}`}function w(t,r){const n=e.output||r||"";return n?B(t,"最近输出","Output",`<pre class="output-pre">${s(n)}</pre>`,!1):""}function it(){return e.activeTab==="containers"?st():e.activeTab==="import"?at():e.activeTab==="diagnostics"?ot():et()}function ct(){return`${C()}${l(e.busy==="refresh"?"刷新中…":"刷新","refresh","ghost",!!e.busy)}${l("检查","check","",!!e.busy)}`}function S(){return'<div class="brand"><span class="logo">AC</span><div><strong>ACHost</strong><small>LXC Panel</small></div></div>'}function d(){g&&(g.innerHTML=`<main class="app-shell lxc-shell">
    <aside class="side-rail">${S()}${k("side-nav")}</aside>
    <section class="workspace">
      <header class="mobile-header">${S()}${C()}</header>
      ${rt("LXC 容器面板")}
      <div class="page-stack">${it()}</div>
    </section>
    ${k("bottom-nav")}
  </main>`)}function ut(t){const r=t.target;if(!(r instanceof HTMLInputElement))return;const n=r.value;r.id==="import-name"&&(e.forms.importName=n),r.id==="rootfs-path"&&(e.forms.rootfsPath=n),r.id==="rootfs-sha256"&&(e.forms.rootfsSha256=n),r.id==="distro"&&(e.forms.distro=n),r.id==="release"&&(e.forms.release=n),r.id==="arch"&&(e.forms.arch=n),r.id==="start-after"&&(e.forms.startAfter=r.checked),r.id==="exec-target"&&(e.forms.execTarget=n),r.id==="exec-command"&&(e.forms.execCommand=n),r.id==="password-target"&&(e.forms.passwordTarget=n),r.id==="password-user"&&(e.forms.passwordUser=n),r.id==="password-value"&&(e.forms.passwordValue=n)}async function lt(t){if(t==="refresh")return v();if(t==="check")return f("运行 LXC 检查","lxc-check",[],!1);if(t==="open-import"){e.activeTab="import",d();return}if(t==="import-rootfs")return J();if(t==="exec"){const r=e.forms.execTarget.trim(),n=e.forms.execCommand.trim();return h(r)?n?f("执行容器命令","lxc-exec",[r,n],!1):u("请输入命令"):u("LXC 容器名不合法")}if(t==="generate-password"){const r=e.forms.passwordTarget.trim(),n=e.forms.passwordUser.trim();return h(r)?X(n)?f("生成密码","lxc-generate-password",[r,n],!1):u("Linux 用户名不合法"):u("LXC 容器名不合法")}if(t==="set-password")return Z()}async function dt(t){const r=t.dataset.container||"",n=t.dataset.containerAction||"";if(!h(r))return u("LXC 容器名不合法");if(n==="start")return f("启动容器","lxc-start",[r]);if(n==="stop")return f("停止容器","lxc-stop",[r]);if(n==="force-stop")return f("强制停止容器","lxc-force-stop",[r]);if(n==="autostart-on")return f("开启容器自启","lxc-set-autostart",[r,"on"]);if(n==="autostart-off")return f("关闭容器自启","lxc-set-autostart",[r,"off"]);if(n==="status")return f("读取容器状态","lxc-system-status",[r],!1);if(n==="logs")return f("读取容器日志","lxc-logs",[r],!1);if(n==="destroy")return window.confirm(`删除 LXC 容器 ${r}？容器 rootfs 和配置都会被删除。`)?f("删除容器","lxc-destroy",[r]):void 0}g&&(g.addEventListener("click",t=>{const r=t.target;if(!(r instanceof HTMLElement))return;const n=r.closest("[data-tab]");if(n){e.activeTab=n.dataset.tab||"dashboard",d();return}const o=r.closest("[data-accordion]");if(o){const c=o.dataset.accordion||"";e.ui.expanded[c]=!N(c),d();return}const a=r.closest("[data-container-action]");if(a){dt(a);return}const i=r.closest("[data-action]");i&&lt(i.dataset.action||"")}),g.addEventListener("input",ut));j();d();v();
