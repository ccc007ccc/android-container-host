(function(){const t=document.createElement("link").relList;if(t&&t.supports&&t.supports("modulepreload"))return;for(const r of document.querySelectorAll('link[rel="modulepreload"]'))d(r);new MutationObserver(r=>{for(const o of r)if(o.type==="childList")for(const p of o.addedNodes)p.tagName==="LINK"&&p.rel==="modulepreload"&&d(p)}).observe(document,{childList:!0,subtree:!0});function n(r){const o={};return r.integrity&&(o.integrity=r.integrity),r.referrerPolicy&&(o.referrerPolicy=r.referrerPolicy),r.crossOrigin==="use-credentials"?o.credentials="include":r.crossOrigin==="anonymous"?o.credentials="omit":o.credentials="same-origin",o}function d(r){if(r.ep)return;r.ep=!0;const o=n(r);fetch(r.href,o)}})();let T=0;function R(e){return`${e}_callback_${Date.now()}_${T++}`}function U(e,t){return typeof t>"u"&&(t={}),new Promise((n,d)=>{const r=R("exec");window[r]=(p,O,z)=>{n({errno:p,stdout:O,stderr:z}),o(r)};function o(p){delete window[p]}try{ksu.exec(e,JSON.stringify(t),r)}catch(p){d(p),o(r)}})}function Z(e){ksu.enableEdgeToEdge(e)}function j(e){ksu.toast(e)}const M="/data/adb/modules/achost-runtime/achost/bin/achost-webui-api.sh",l=document.querySelector("#app");let _=M,v="dashboard",a=null,w=[],y=[],$="",c="",S="",E="",L="",A="",D="",q="",C="bridge",I="";try{Z(!0)}catch{}function s(e){return String(e??"").replace(/&/g,"&amp;").replace(/</g,"&lt;").replace(/>/g,"&gt;").replace(/"/g,"&quot;").replace(/'/g,"&#39;")}function H(e){return`'${e.replace(/'/g,"'\\''")}'`}function F(e){return/^[A-Za-z0-9_.-]+$/.test(e)}function N(e){return/^[A-Za-z0-9_./:@-]+$/.test(e)}function k(e,t){return!e.trim()||e.split(",").every(n=>t.test(n.trim()))}function g(e){try{j(e)}catch{}}async function J(){try{const e=await fetch("./achost-webui-config.json",{cache:"no-store"});if(!e.ok)return;const t=await e.json();typeof t.api=="string"&&t.api.startsWith("/data/adb/modules/")&&(_=t.api)}catch{}}async function h(e,t=[]){const n=[_,e,...t].map(H).join(" "),d=await U(n),r=d.stdout.trim()||d.stderr.trim();if(!r)return{ok:!1,error:`命令没有输出，errno=${d.errno}`};try{const o=JSON.parse(r);return d.errno!==0&&o.ok!==!1&&(o.ok=!1,o.error=`errno=${d.errno}`),o}catch{return{ok:!1,error:r}}}function x(e){return typeof e.output=="string"?e.output:typeof e.error=="string"?e.error:JSON.stringify(e,null,2)}async function P(){c="refresh",f();try{const[e,t,n]=await Promise.all([h("status"),h("list-containers"),h("list-images")]);a=e,w=Array.isArray(t.containers)?t.containers:[],y=Array.isArray(n.images)?n.images:[],e.ok===!1&&($=x(e))}catch(e){$=e instanceof Error?e.message:String(e),g("刷新失败")}finally{c="",f()}}async function u(e,t,n=[],d=!0){c=t,f();try{const r=await h(t,n);$=x(r),g(r.ok===!1?`${e}失败`:`${e}完成`),d&&await P()}catch(r){$=r instanceof Error?r.message:String(r),g(`${e}失败`)}finally{c="",f()}}async function K(){const e=E.trim(),t=L.trim(),n=A.trim(),d=D.trim(),r=q.trim(),o=C.trim()||"bridge";if(!F(e)){g("容器名只能包含字母、数字、点、下划线和短横线");return}if(!N(t)){g("镜像名包含不支持的字符");return}if(!k(n,/^[0-9:/a-z.-]+$/)){g("端口映射格式不合法");return}if(!k(d,/^[A-Za-z_][A-Za-z0-9_=@.,:/+-]*$/)){g("环境变量格式不合法");return}if(!k(r,/^\/[A-Za-z0-9_./:@,+=-]+:\/[A-Za-z0-9_./:@,+=-]+$/)){g("挂载格式不合法");return}await u("创建容器","add-container",[e,t,n,d,r,o])}async function W(){const e=I.trim();if(!N(e)){g("镜像名包含不支持的字符");return}await u("拉取镜像","pull-image",[e])}async function G(e){await u(e?"开启自启":"关闭自启","set-autostart",[e?"on":"off"])}async function Q(){v="diagnostics",await u("运行检查","check",[],!1)}function Y(e){return e.status.toLowerCase().startsWith("up")}function B(){const e=!!a?.running;return`<span class="pill ${e?"ok":"stop"}"><span></span>${e?"Docker 运行中":"Docker 已停止"}</span>`}function b(e,t,n=""){return`<section class="card stat ${n}"><span>${e}</span><strong>${s(t)}</strong></section>`}function i(e,t){return`<div class="detail-item"><span>${e}</span><strong>${s(t||"—")}</strong></div>`}function m(e,t){return`<button class="nav-item ${v===e?"active":""}" data-tab="${e}">${t}</button>`}function V(){return`
    <section class="grid stats-grid">
      ${b("容器总数",a?.containers_total??"—")}
      ${b("运行中",a?.containers_running??"—","green")}
      ${b("已停止",a?.containers_stopped??"—","amber")}
      ${b("镜像",a?.images??(y.length||"—"))}
    </section>

    <section class="panel">
      <div class="section-head">
        <div>
          <p class="eyebrow">Runtime</p>
          <h2>运行时概览</h2>
        </div>
        ${B()}
      </div>
      <div class="detail-grid">
        ${i("Docker 版本",a?.server_version)}
        ${i("Storage Driver",a?.storage_driver)}
        ${i("Cgroup",a?.cgroup_version)}
        ${i("dockerd pid",a?.dockerd_pid)}
        ${i("containerd pid",a?.containerd_pid)}
        ${i("Socket",a?.socket?"ready":"missing")}
        ${i("Base 模块",a?.base_present?"present":"missing")}
        ${i("Data root",a?.data_root)}
        ${i("开机自启",a?.autostart?"已开启":"未开启")}
      </div>
    </section>
  `}function X(){const e=S.trim().toLowerCase();return e?w.filter(t=>[t.name,t.id,t.image,t.status].some(n=>n.toLowerCase().includes(e))):w}function ee(){const e=X();return e.length?e.map(t=>{const n=t.id||t.name,d=Y(t);return`
        <article class="table-row container-row">
          <div class="identity-cell">
            <strong title="${s(t.name||t.id)}">${s(t.name||"(无名称)")}</strong>
            <small title="${s(t.id)}">ID: ${s(t.id)}</small>
          </div>
          <div class="meta-cell">
            <span title="${s(t.image)}">${s(t.image)}</span>
            <small title="${s(t.created)}">创建时间: ${s(t.created)}</small>
          </div>
          <div><span class="badge ${d?"green":"slate"}">${s(t.status)}</span></div>
          <div class="row-actions">
            <button class="small" data-container-action="start" data-target="${s(n)}" ${c||d?"disabled":""}>启动</button>
            <button class="small" data-container-action="stop" data-target="${s(n)}" ${c||!d?"disabled":""}>停止</button>
            <button class="small" data-container-action="restart" data-target="${s(n)}" ${c?"disabled":""}>重启</button>
            <button class="small ghost" data-container-action="logs" data-target="${s(n)}" ${c?"disabled":""}>日志</button>
            <button class="small ghost" data-container-action="inspect" data-target="${s(n)}" ${c?"disabled":""}>Inspect</button>
            <button class="small danger" data-container-action="delete" data-target="${s(n)}" ${c?"disabled":""}>删除</button>
          </div>
        </article>`}).join(""):'<div class="empty">没有匹配的容器。可以用下方表单创建 detached 容器。</div>'}function te(){return`
    <section class="panel">
      <div class="section-head split">
        <div>
          <p class="eyebrow">Containers</p>
          <h2>容器管理</h2>
        </div>
        <input class="search" id="container-search" value="${s(S)}" placeholder="搜索名称、镜像或状态" />
      </div>
      <div class="table-list">${ee()}</div>
    </section>

    <section class="panel create-panel">
      <div>
        <p class="eyebrow">Run detached</p>
        <h2>创建容器</h2>
      </div>
      <div class="form-grid">
        <label>容器名<input id="name" value="${s(E)}" placeholder="demo-nginx" /></label>
        <label>镜像名<input id="image" value="${s(L)}" placeholder="nginx:alpine" /></label>
        <label>网络<input id="network" value="${s(C)}" placeholder="bridge" /></label>
        <label>端口映射<input id="ports" value="${s(A)}" placeholder="8080:80,8443:443" /></label>
        <label>环境变量<input id="envs" value="${s(D)}" placeholder="KEY=value,DEBUG=1" /></label>
        <label>Bind mount<input id="mounts" value="${s(q)}" placeholder="/sdcard/www:/usr/share/nginx/html" /></label>
      </div>
      <button data-action="add" ${c?"disabled":""}>创建容器</button>
    </section>
  `}function ae(e){return e.repository==="<none>"||e.tag==="<none>"?e.id:`${e.repository}:${e.tag}`}function ne(){const e=y.length?y.map(t=>`
          <article class="table-row image-row">
            <div>
              <strong>${s(ae(t))}</strong>
              <small>${s(t.id)}</small>
            </div>
            <div><span>${s(t.size)}</span><small>${s(t.created)}</small></div>
            <div class="row-actions">
              <button class="small danger" data-remove-image="${s(t.id)}" ${c?"disabled":""}>删除镜像</button>
            </div>
          </article>`).join(""):'<div class="empty">暂无镜像。可以先拉取镜像，或从命令行 import 本地 rootfs。</div>';return`
    <section class="panel create-panel">
      <div>
        <p class="eyebrow">Images</p>
        <h2>拉取镜像</h2>
      </div>
      <label>镜像名<input id="pull-image" value="${s(I)}" placeholder="alpine:latest" /></label>
      <button data-action="pull-image" ${c?"disabled":""}>Pull</button>
    </section>
    <section class="panel">
      <div class="section-head">
        <div>
          <p class="eyebrow">Local images</p>
          <h2>本地镜像</h2>
        </div>
      </div>
      <div class="table-list">${e}</div>
    </section>
  `}function re(){return`
    <section class="panel actions-panel">
      <div>
        <p class="eyebrow">Diagnostics</p>
        <h2>诊断与日志</h2>
      </div>
      <div class="button-row">
        <button data-action="check" ${c?"disabled":""}>运行 runtime check</button>
        <button data-action="daemon-logs" ${c?"disabled":""}>查看 daemon 日志</button>
        <button data-action="refresh" ${c?"disabled":""}>刷新状态</button>
      </div>
    </section>
    <section class="panel">
      <div class="section-head">
        <div>
          <p class="eyebrow">Android compatibility</p>
          <h2>非常规环境状态</h2>
        </div>
        <span class="badge ${a?.route_status==="ok"?"green":"slate"}">路由 ${s(a?.route_status||"unknown")}</span>
      </div>
      <div class="detail-grid diagnostics-grid">
        ${i("Runtime mode",a?.runtime_mode)}
        ${i("Cgroup mode",a?.configured_cgroup_mode||a?.cgroup_version)}
        ${i("Host cgroup",a?.cgroup_mount)}
        ${i("DNS servers",a?.dns_servers||a?.resolv_nameservers)}
        ${i("resolv.conf",a?.resolv_conf)}
        ${i("resolv nameservers",a?.resolv_nameservers)}
        ${i("Bridge",a?.bridge)}
        ${i("Bridge subnet",a?.bridge_subnet)}
        ${i("Bridge route",a?.bridge_route)}
        ${i("Return policy",a?.return_policy_rule)}
        ${i("Source policy",a?.source_policy_rule)}
        ${i("Uplink",a?.uplink)}
      </div>
    </section>
    <section class="panel output-panel">
      <div class="section-head">
        <h2>输出</h2>
      </div>
      <pre>${s($||a?.docker_error||"暂无输出")}</pre>
    </section>
  `}function se(){const e=!!a?.autostart;return`
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
        <button class="switch ${e?"on":""}" data-action="toggle-autostart" ${c?"disabled":""}>
          <span></span>${e?"已开启":"已关闭"}
        </button>
      </div>
      <div class="detail-grid compact">
        ${i("API",_)}
        ${i("Data root",a?.data_root)}
        ${i("Autostart file",a?.autostart_file)}
        ${i("Base 模块",a?.base_present?"present":"missing")}
      </div>
    </section>
  `}function ie(){return v==="containers"?te():v==="images"?ne():v==="diagnostics"?re():v==="settings"?se():V()}function f(){l.innerHTML=`
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
          ${m("dashboard","Dashboard")}
          ${m("containers","Containers")}
          ${m("images","Images")}
          ${m("diagnostics","Diagnostics")}
          ${m("settings","Settings")}
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
            ${B()}
            <button data-action="refresh" ${c?"disabled":""}>${c==="refresh"?"刷新中…":"刷新"}</button>
            <button data-action="start" ${c||a?.running?"disabled":""}>启动</button>
            <button class="danger" data-action="stop" ${c||!a?.running?"disabled":""}>停止</button>
          </div>
        </header>
        ${ie()}
      </section>
    </main>
  `,l.querySelectorAll("[data-tab]").forEach(e=>{e.addEventListener("click",()=>{v=e.dataset.tab,f()})}),l.querySelector('[data-action="refresh"]')?.addEventListener("click",()=>P()),l.querySelector('[data-action="start"]')?.addEventListener("click",()=>u("启动 Docker","start-docker")),l.querySelector('[data-action="stop"]')?.addEventListener("click",()=>u("停止 Docker","stop-docker")),l.querySelector('[data-action="check"]')?.addEventListener("click",()=>Q()),l.querySelector('[data-action="daemon-logs"]')?.addEventListener("click",()=>u("读取日志","daemon-logs",[],!1)),l.querySelector('[data-action="add"]')?.addEventListener("click",()=>K()),l.querySelector('[data-action="pull-image"]')?.addEventListener("click",()=>W()),l.querySelector('[data-action="toggle-autostart"]')?.addEventListener("click",()=>G(!a?.autostart)),l.querySelector("#container-search")?.addEventListener("input",e=>{S=e.target.value,f()}),l.querySelector("#name")?.addEventListener("input",e=>{E=e.target.value}),l.querySelector("#image")?.addEventListener("input",e=>{L=e.target.value}),l.querySelector("#network")?.addEventListener("input",e=>{C=e.target.value}),l.querySelector("#ports")?.addEventListener("input",e=>{A=e.target.value}),l.querySelector("#envs")?.addEventListener("input",e=>{D=e.target.value}),l.querySelector("#mounts")?.addEventListener("input",e=>{q=e.target.value}),l.querySelector("#pull-image")?.addEventListener("input",e=>{I=e.target.value}),l.querySelectorAll("[data-container-action]").forEach(e=>{e.addEventListener("click",()=>{const t=e.dataset.containerAction||"",n=e.dataset.target||"";n&&(t==="logs"&&u("读取容器日志","container-logs",[n],!1),t==="inspect"&&u("Inspect 容器","inspect-container",[n],!1),t==="delete"&&u("删除容器","delete-container",[n]),t==="start"&&u("启动容器","start-container",[n]),t==="stop"&&u("停止容器","stop-container",[n]),t==="restart"&&u("重启容器","restart-container",[n]))})}),l.querySelectorAll("[data-remove-image]").forEach(e=>{e.addEventListener("click",()=>{const t=e.dataset.removeImage||"";t&&u("删除镜像","remove-image",[t])})})}f();J().finally(()=>P());
