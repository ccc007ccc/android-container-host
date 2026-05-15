(function(){const s=document.createElement("link").relList;if(s&&s.supports&&s.supports("modulepreload"))return;for(const a of document.querySelectorAll('link[rel="modulepreload"]'))o(a);new MutationObserver(a=>{for(const i of a)if(i.type==="childList")for(const c of i.addedNodes)c.tagName==="LINK"&&c.rel==="modulepreload"&&o(c)}).observe(document,{childList:!0,subtree:!0});function r(a){const i={};return a.integrity&&(i.integrity=a.integrity),a.referrerPolicy&&(i.referrerPolicy=a.referrerPolicy),a.crossOrigin==="use-credentials"?i.credentials="include":a.crossOrigin==="anonymous"?i.credentials="omit":i.credentials="same-origin",i}function o(a){if(a.ep)return;a.ep=!0;const i=r(a);fetch(a.href,i)}})();let E=0;function F(t){return`${t}_callback_${Date.now()}_${E++}`}function T(t,s){return typeof s>"u"&&(s={}),new Promise((r,o)=>{const a=F("exec");window[a]=(c,m,p)=>{r({errno:c,stdout:m,stderr:p}),i(a)};function i(c){delete window[c]}try{ksu.exec(t,JSON.stringify(s),a)}catch(c){o(c),i(a)}})}function I(t){ksu.enableEdgeToEdge(t)}function U(t){ksu.toast(t)}const H="/data/adb/modules/achost-lxc/achost/bin/achost-webui-api.sh",h=document.querySelector("#app"),e={apiPath:H,activeTab:"dashboard",busy:"",status:null,containers:[],output:"",ui:{expanded:{},sheet:{kind:"none"}},forms:{importName:"ubuntu-26.04",rootfsPath:"",rootfsSha256:"",distro:"ubuntu",release:"26.04",arch:"arm64",startAfter:!1,execTarget:"",execCommand:"cat /etc/os-release || uname -a",passwordTarget:"ubuntu-26.04",passwordUser:"root",passwordValue:""}};try{I(!0)}catch{}function j(){const t=document.querySelector('meta[name="achost-webui-config"]'),s=t?t.content:"";if(s)try{const r=JSON.parse(s);typeof r.api=="string"&&r.api.startsWith("/data/adb/modules/")&&(e.apiPath=r.api)}catch{}}function n(t){return String(t??"").replace(/&/g,"&amp;").replace(/</g,"&lt;").replace(/>/g,"&gt;").replace(/"/g,"&quot;").replace(/'/g,"&#39;")}function P(t){return`'${t.replace(/'/g,"'\\''")}'`}function u(t){try{U(t)}catch{}}function M(){return new Promise(t=>requestAnimationFrame(()=>t()))}function g(t){return/^[A-Za-z0-9_.-]+$/.test(t)&&t!=="."&&t!==".."&&t.indexOf("..")===-1}function R(t){return/^[A-Za-z0-9_.-]+$/.test(t)}function W(t){return t.startsWith("/")&&!/[\x00-\x1F\x7F]/.test(t)}function D(t){return/^[A-Fa-f0-9]{64}$/.test(t)}function X(t){return/^[A-Za-z_][A-Za-z0-9_.-]{0,63}$/.test(t)}function q(t){return t.length>0&&!/[\x00-\x1F\x7F:\r\n]/.test(t)}async function $(t,s=[]){const r=[e.apiPath,t].concat(s).map(P).join(" "),o=await T(r);return _(o)}async function z(t,s,r){const o=[e.apiPath,t].concat(s).map(P).join(" "),i=await T(o,{env:r});return _(i)}function _(t){const s=t.stdout.trim()||t.stderr.trim();if(!s)return{ok:!1,error:`命令没有输出，errno=${t.errno}`};try{const r=JSON.parse(s);return t.errno!==0&&r.ok!==!1&&(r.ok=!1,r.error=`errno=${t.errno}`),r}catch{return{ok:!1,error:s}}}function b(t){return typeof t.output=="string"?t.output:typeof t.error=="string"?t.error:JSON.stringify(t,null,2)}function V(t){return Array.isArray(t)?t.map(s=>{const r=s;return{name:String(r.name||""),state:String(r.state||"UNKNOWN"),pid:typeof r.pid=="string"?r.pid:"",distro:String(r.distro||"unknown"),release:String(r.release||"unknown"),arch:String(r.arch||"unknown"),rootfs:String(r.rootfs||""),config:String(r.config||""),log:String(r.log||""),autostart:!!r.autostart}}).filter(s=>s.name):[]}function x(t){return t.state.toLowerCase()==="running"}function y(t,s){if(typeof t=="number")return t;if(typeof t=="string"&&t){const r=Number(t);if(Number.isFinite(r))return r}return s}async function v(){e.busy="refresh",d();try{const t=await $("lxc-status"),s=await $("lxc-list");e.status=t,e.containers=V(s.containers||t.containers),t.ok===!1&&(e.output=b(t)),s.ok===!1&&(e.output=b(s))}catch(t){e.output=t instanceof Error?t.message:String(t),u("刷新失败")}finally{e.busy="",d()}}async function f(t,s,r=[],o=!0){e.busy=s,d(),await M();try{const a=await $(s,r),i=a.ok===!1;e.output=b(a),u(i?`${t}失败`:`${t}完成`),o&&await v(),i||!o?e.ui.sheet={kind:"output",title:i?`${t}失败`:t}:e.ui.sheet.kind!=="none"&&(e.ui.sheet={kind:"none"})}catch(a){e.output=a instanceof Error?a.message:String(a),e.ui.sheet={kind:"output",title:`${t}失败`},u(`${t}失败`)}finally{e.busy="",d()}}async function J(){const t=e.forms.importName.trim(),s=e.forms.rootfsPath.trim(),r=e.forms.rootfsSha256.trim(),o=e.forms.distro.trim()||"unknown",a=e.forms.release.trim()||"unknown",i=e.forms.arch.trim()||"unknown";if(!g(t))return u("LXC 容器名不合法");if(!W(s))return u("rootfs 路径必须是 Android 绝对路径");if(r&&!D(r))return u("SHA-256 必须是 64 位十六进制");if(![o,a,i].every(R))return u("rootfs 元数据只能包含字母、数字、点、下划线和短横线");e.busy="lxc-import-rootfs",d();const c=[];let m=!1;try{const p=[t,s,o,a,i];r&&p.push(r.toLowerCase());const C=await $("lxc-import-rootfs",p);if(m=C.ok===!1,c.push(`## 导入 rootfs
${b(C)}`),!m&&e.forms.startAfter){const A=await $("lxc-start",[t]);m=A.ok===!1,c.push(`## 启动容器
${b(A)}`)}e.output=c.join(`

`),m&&(e.ui.sheet={kind:"output",title:"导入 rootfs 失败"}),u(m?"导入 rootfs 失败":"导入 rootfs 完成"),await v()}catch(p){e.output=c.concat(p instanceof Error?p.message:String(p)).join(`

`),e.ui.sheet={kind:"output",title:"导入 rootfs 失败"},u("导入 rootfs 失败")}finally{e.busy="",d()}}async function Z(){const t=e.forms.passwordTarget.trim(),s=e.forms.passwordUser.trim(),r=e.forms.passwordValue;if(!g(t))return u("LXC 容器名不合法");if(!X(s))return u("Linux 用户名不合法");if(!q(r))return u("密码不能为空，且不能包含冒号、换行或控制字符");e.busy="lxc-set-password",d();try{const o=await z("lxc-set-password",[t,s],{ACHOST_LXC_PASSWORD:r}),a=o.ok===!1;e.output=b(o),e.forms.passwordValue="",a&&(e.ui.sheet={kind:"output",title:"设置密码失败"}),u(a?"设置密码失败":"设置密码完成")}catch(o){e.output=o instanceof Error?o.message:String(o),e.forms.passwordValue="",e.ui.sheet={kind:"output",title:"设置密码失败"},u("设置密码失败")}finally{e.busy="",d()}}function l(t,s,r="",o=!1){return`<button type="button" class="${n(r)}" data-action="${n(s)}" ${o?"disabled":""}>${n(t)}</button>`}function K(){return[{tab:"dashboard",label:"概览"},{tab:"containers",label:"容器"},{tab:"import",label:"导入"},{tab:"diagnostics",label:"诊断"}]}function L(t){return`<nav class="${n(t)}" aria-label="主导航">${K().map(s=>`<button type="button" class="nav-item ${e.activeTab===s.tab?"active":""}" data-tab="${s.tab}" ${e.activeTab===s.tab?'aria-current="page"':""}><span>${n(s.label)}</span></button>`).join("")}</nav>`}function k(){if(!e.status)return'<span class="pill stop"><span></span>LXC 未知</span>';if(e.status.ok===!1)return'<span class="pill stop"><span></span>LXC 异常</span>';const t=y(e.status.containers_running,e.containers.filter(x).length);return`<span class="pill ok"><span></span>LXC · ${n(t)} 运行</span>`}function G(t,s,r=""){return`<article class="metric-card ${r}"><span>${n(t)}</span><strong>${n(s)}</strong></article>`}function Q(t){return`<section class="metric-strip">${t.map(([s,r,o])=>G(s,r,o)).join("")}</section>`}function Y(t,s){return`<div class="detail-item"><span>${n(t)}</span><strong>${n(s||"—")}</strong></div>`}function tt(t){return`<div class="detail-grid">${t.map(([s,r])=>Y(s,r)).join("")}</div>`}function B(t,s=!1){return e.ui.expanded[t]??s}function N(t,s,r,o,a=!1,i=""){const c=B(t,a);return`
    <section class="panel accordion ${c?"open":""}">
      <button type="button" class="accordion-trigger" data-accordion="${n(t)}" aria-expanded="${c?"true":"false"}">
        <span>
          <small>${n(r)}</small>
          <strong>${n(s)}</strong>
        </span>
        <span class="accordion-side">${i}<span class="chevron">${c?"收起":"展开"}</span></span>
      </button>
      ${c?`<div class="accordion-body">${o}</div>`:""}
    </section>`}function et(t){return`
    <section class="page-intro">
      <div>
        <p class="eyebrow">KernelSU Module WebUI</p>
        <h1>${n(t)}</h1>
      </div>
      <div class="top-actions">${ct()}</div>
    </section>`}function O(t,s){return`
    <section class="panel command-panel">
      <div>
        <p class="eyebrow">Control</p>
        <h2>${n(t)}</h2>
      </div>
      <div class="button-row">${s}</div>
    </section>`}function st(){const t=e.status,s=e.containers.filter(r=>r.autostart).length;return`
    ${Q([["容器",y(t?.containers_total,e.containers.length||"—")],["运行",y(t?.containers_running,e.containers.filter(x).length),"success"],["停止",y(t?.containers_stopped,"—"),"warning"],["自启",s||"—"]])}
    ${O("LXC 控制台",`${l("刷新","refresh","ghost",!!e.busy)}${l("运行检查","check","",!!e.busy)}${l("导入 rootfs","open-import","ghost",!!e.busy)}`)}
    ${N("lxc-runtime","基础模块详情","LXC Runtime",tt([["LXC Runtime",t?.lxc_runtime],["容器目录",t?.lxc_containers],["Bridge",t?.bridge||"lxcbr0"],["Bridge subnet",t?.bridge_subnet],["Base 模块",t?.base_present?"present":"missing"],["Module target",t?.module_target||"lxc"],["Data root",t?.data_root],["API",e.apiPath]]),!1,k())}
    ${w("lxc-output")}
  `}function rt(){return e.containers.length?e.containers.map(t=>{const s=x(t),r=s?"stop":"start",o=t.autostart?"autostart-off":"autostart-on";return`<article class="entity-card lxc-container-card">
        <div class="lxc-card-head">
          <div class="lxc-card-title">
            <strong title="${n(t.name)}">${n(t.name)}</strong>
            <small>${n(`${t.distro} ${t.release} / ${t.arch}`)}</small>
          </div>
          <div class="lxc-card-badges">
            <span class="badge ${s?"green":"slate"}">${n(t.state)}${t.pid?` · pid ${n(t.pid)}`:""}</span>
            <span class="badge ${t.autostart?"green":"slate"}">自启 ${t.autostart?"on":"off"}</span>
          </div>
        </div>
        <div class="lxc-card-details">
          <div class="lxc-card-detail"><b>rootfs</b><span title="${n(t.rootfs)}">${n(t.rootfs||"—")}</span></div>
          <div class="lxc-card-detail"><b>config</b><span title="${n(t.config)}">${n(t.config||"—")}</span></div>
        </div>
        <div class="lxc-card-actions">
          <button type="button" class="small" data-container="${n(t.name)}" data-container-action="${r}" ${e.busy?"disabled":""}>${s?"停止":"启动"}</button>
          ${s?`<button type="button" class="small danger" data-container="${n(t.name)}" data-container-action="force-stop" ${e.busy?"disabled":""}>强制停止</button>`:""}
          <button type="button" class="small ghost" data-container="${n(t.name)}" data-container-action="${o}" ${e.busy?"disabled":""}>自启${t.autostart?"关":"开"}</button>
          <button type="button" class="small ghost" data-container="${n(t.name)}" data-container-action="status" ${e.busy?"disabled":""}>状态</button>
          <button type="button" class="small ghost" data-container="${n(t.name)}" data-container-action="logs" ${e.busy?"disabled":""}>日志</button>
          <button type="button" class="small danger" data-container="${n(t.name)}" data-container-action="destroy" ${e.busy?"disabled":""}>删除</button>
        </div>
      </article>`}).join(""):'<div class="empty">暂无 LXC 容器。先在导入页导入 rootfs。</div>'}function nt(){return`<section class="panel list-tools"><div><p class="eyebrow">Inventory</p><h2>LXC 容器</h2></div>${l("刷新","refresh","ghost",!!e.busy)}</section><section class="entity-list">${rt()}</section>${w("lxc-container-output")}`}function at(){return`<section class="panel form-panel">
    <div><p class="eyebrow">Import rootfs</p><h2>导入 LXC rootfs</h2></div>
    <p class="form-note">先通过 adb 或文件管理器把 rootfs tar/tar.gz 放到设备绝对路径，再在这里导入。SHA-256 填写时会先校验。</p>
    <div class="form-grid primary-form">
      <label>容器名<input id="import-name" value="${n(e.forms.importName)}" placeholder="ubuntu-26.04" /></label>
      <label>rootfs 路径<input id="rootfs-path" value="${n(e.forms.rootfsPath)}" placeholder="/data/local/tmp/ubuntu-26.04-arm64-rootfs.tar.gz" /></label>
    </div>
    <div class="form-grid">
      <label>rootfs SHA-256<input id="rootfs-sha256" value="${n(e.forms.rootfsSha256)}" placeholder="可选" /></label>
      <label>Distro<input id="distro" value="${n(e.forms.distro)}" /></label>
      <label>Release<input id="release" value="${n(e.forms.release)}" /></label>
      <label>Arch<input id="arch" value="${n(e.forms.arch)}" /></label>
    </div>
    <div class="option-stack">
      <label class="check-row"><input id="start-after" type="checkbox" ${e.forms.startAfter?"checked":""} /><span>导入成功后启动容器</span></label>
    </div>
    <div class="button-row">${l("导入 rootfs","import-rootfs","",!!e.busy)}</div>
  </section>${w("lxc-import-output")}`}function ot(){return`${O("诊断与维护",`${l("运行 LXC 检查","check","",!!e.busy)}${l("刷新状态","refresh","ghost",!!e.busy)}`)}
  <section class="panel form-panel"><div><p class="eyebrow">Exec</p><h2>容器命令</h2></div><div class="form-grid"><label>容器名<input id="exec-target" value="${n(e.forms.execTarget)}" /></label><label>命令<input id="exec-command" value="${n(e.forms.execCommand)}" /></label></div><div class="button-row">${l("执行","exec","",!!e.busy)}</div></section>
  <section class="panel form-panel"><div><p class="eyebrow">Password</p><h2>用户密码</h2></div><p class="form-note warning-note">直接更新容器 rootfs 的 /etc/shadow；只用于可信本机管理场景。</p><div class="form-grid"><label>容器名<input id="password-target" value="${n(e.forms.passwordTarget)}" /></label><label>Linux 用户<input id="password-user" value="${n(e.forms.passwordUser)}" /></label><label>密码<input id="password-value" type="password" value="${n(e.forms.passwordValue)}" /></label></div><div class="button-row">${l("生成密码","generate-password","ghost",!!e.busy)}${l("设置密码","set-password","",!!e.busy)}</div></section>
  ${w("lxc-diagnostics-output",e.status?.error)}`}function w(t,s){const r=e.output||s||"";return r?N(t,"最近输出","Output",`<pre class="output-pre">${n(r)}</pre><div class="button-row compact"><button type="button" class="ghost" data-action="output">打开输出面板</button></div>`,!1):""}function it(){return e.activeTab==="containers"?nt():e.activeTab==="import"?at():e.activeTab==="diagnostics"?ot():st()}function ct(){const t=e.output?l("输出","output","ghost",!1):"";return`${k()}${l(e.busy==="refresh"?"刷新中…":"刷新","refresh","ghost",!!e.busy)}${l("检查","check","",!!e.busy)}${t}`}function S(){return'<div class="brand"><span class="logo">AC</span><div><strong>ACHost</strong><small>LXC Panel</small></div></div>'}function d(){h&&(h.innerHTML=`<main class="app-shell lxc-shell">
    <aside class="side-rail">${S()}${L("side-nav")}</aside>
    <section class="workspace">
      <header class="mobile-header">${S()}${k()}</header>
      ${et("LXC 容器面板")}
      <div class="page-stack">${it()}</div>
    </section>
    ${L("bottom-nav")}
    ${ut()}
  </main>`)}function ut(){return e.ui.sheet.kind==="none"?"":`
    <div class="sheet-layer" data-backdrop="sheet">
      <section class="sheet" role="dialog" aria-modal="true" aria-labelledby="sheet-title">
        <div class="sheet-handle"></div>
        <header class="sheet-head">
          <div>
            <p class="eyebrow">Action sheet</p>
            <h2 id="sheet-title">${n(e.ui.sheet.title)}</h2>
          </div>
          <button type="button" class="icon-button ghost" data-action="close-sheet" aria-label="关闭">关闭</button>
        </header>
        <div class="sheet-body"><pre class="output-pre sheet-output">${n(e.output||"暂无输出")}</pre></div>
        <footer class="sheet-footer">${l("关闭","close-sheet","ghost",!1)}</footer>
      </section>
    </div>`}function lt(t){const s=t.target;if(!(s instanceof HTMLInputElement))return;const r=s.value;s.id==="import-name"&&(e.forms.importName=r),s.id==="rootfs-path"&&(e.forms.rootfsPath=r),s.id==="rootfs-sha256"&&(e.forms.rootfsSha256=r),s.id==="distro"&&(e.forms.distro=r),s.id==="release"&&(e.forms.release=r),s.id==="arch"&&(e.forms.arch=r),s.id==="start-after"&&(e.forms.startAfter=s.checked),s.id==="exec-target"&&(e.forms.execTarget=r),s.id==="exec-command"&&(e.forms.execCommand=r),s.id==="password-target"&&(e.forms.passwordTarget=r),s.id==="password-user"&&(e.forms.passwordUser=r),s.id==="password-value"&&(e.forms.passwordValue=r)}async function dt(t){if(t==="close-sheet"){e.ui.sheet={kind:"none"},d();return}if(t==="output"){e.ui.sheet={kind:"output",title:"输出"},d();return}if(t==="refresh")return v();if(t==="check")return f("运行 LXC 检查","lxc-check",[],!1);if(t==="open-import"){e.activeTab="import",d();return}if(t==="import-rootfs")return J();if(t==="exec"){const s=e.forms.execTarget.trim(),r=e.forms.execCommand.trim();return g(s)?r?f("执行容器命令","lxc-exec",[s,r],!1):u("请输入命令"):u("LXC 容器名不合法")}if(t==="generate-password"){const s=e.forms.passwordTarget.trim(),r=e.forms.passwordUser.trim();return g(s)?X(r)?f("生成密码","lxc-generate-password",[s,r],!1):u("Linux 用户名不合法"):u("LXC 容器名不合法")}if(t==="set-password")return Z()}async function ft(t){const s=t.dataset.container||"",r=t.dataset.containerAction||"";if(!g(s))return u("LXC 容器名不合法");if(r==="start")return f("启动容器","lxc-start",[s]);if(r==="stop")return f("停止容器","lxc-stop",[s]);if(r==="force-stop")return f("强制停止容器","lxc-force-stop",[s]);if(r==="autostart-on")return f("开启容器自启","lxc-set-autostart",[s,"on"]);if(r==="autostart-off")return f("关闭容器自启","lxc-set-autostart",[s,"off"]);if(r==="status")return f("读取容器状态","lxc-system-status",[s],!1);if(r==="logs")return f("读取容器日志","lxc-logs",[s],!1);if(r==="destroy")return window.confirm(`删除 LXC 容器 ${s}？容器 rootfs 和配置都会被删除。`)?f("删除容器","lxc-destroy",[s]):void 0}h&&(h.addEventListener("click",t=>{const s=t.target;if(!(s instanceof HTMLElement))return;const r=s.closest("[data-tab]");if(r){e.activeTab=r.dataset.tab||"dashboard",d();return}const o=s.closest("[data-accordion]");if(o){const c=o.dataset.accordion||"";e.ui.expanded[c]=!B(c),d();return}const a=s.closest("[data-container-action]");if(a){ft(a);return}const i=s.closest("[data-action]");i&&dt(i.dataset.action||"")}),h.addEventListener("input",lt));j();d();v();
