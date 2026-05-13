(function(){const e=document.createElement("link").relList;if(e&&e.supports&&e.supports("modulepreload"))return;for(const n of document.querySelectorAll('link[rel="modulepreload"]'))l(n);new MutationObserver(n=>{for(const i of n)if(i.type==="childList")for(const p of i.addedNodes)p.tagName==="LINK"&&p.rel==="modulepreload"&&l(p)}).observe(document,{childList:!0,subtree:!0});function a(n){const i={};return n.integrity&&(i.integrity=n.integrity),n.referrerPolicy&&(i.referrerPolicy=n.referrerPolicy),n.crossOrigin==="use-credentials"?i.credentials="include":n.crossOrigin==="anonymous"?i.credentials="omit":i.credentials="same-origin",i}function l(n){if(n.ep)return;n.ep=!0;const i=a(n);fetch(n.href,i)}})();let B=0;function Z(t){return`${t}_callback_${Date.now()}_${B++}`}function j(t,e){return typeof e>"u"&&(e={}),new Promise((a,l)=>{const n=Z("exec");window[n]=(p,z,T)=>{a({errno:p,stdout:z,stderr:T}),i(n)};function i(p){delete window[p]}try{ksu.exec(t,JSON.stringify(e),n)}catch(p){l(p),i(n)}})}function M(t){ksu.enableEdgeToEdge(t)}function U(t){ksu.toast(t)}const R="/data/adb/modules/achost-runtime/achost/bin/achost-webui-api.sh",c=document.querySelector("#app");let S=R,f="dashboard",s=null,k=[],y=[],b="",o="",E="",L="",A="",_="",q="",D="",C="bridge",I="";try{M(!0)}catch{}function r(t){return String(t??"").replace(/&/g,"&amp;").replace(/</g,"&lt;").replace(/>/g,"&gt;").replace(/"/g,"&quot;").replace(/'/g,"&#39;")}function F(t){return`'${t.replace(/'/g,"'\\''")}'`}function H(t){return/^[A-Za-z0-9_.-]+$/.test(t)}function N(t){return/^[A-Za-z0-9_./:@-]+$/.test(t)}function w(t,e){return!t.trim()||t.split(",").every(a=>e.test(a.trim()))}function g(t){try{U(t)}catch{}}async function J(){try{const t=await fetch("./achost-webui-config.json",{cache:"no-store"});if(!t.ok)return;const e=await t.json();typeof e.api=="string"&&e.api.startsWith("/data/adb/modules/")&&(S=e.api)}catch{}}async function h(t,e=[]){const a=[S,t,...e].map(F).join(" "),l=await j(a),n=l.stdout.trim()||l.stderr.trim();if(!n)return{ok:!1,error:`命令没有输出，errno=${l.errno}`};try{const i=JSON.parse(n);return l.errno!==0&&i.ok!==!1&&(i.ok=!1,i.error=`errno=${l.errno}`),i}catch{return{ok:!1,error:n}}}function x(t){return typeof t.output=="string"?t.output:typeof t.error=="string"?t.error:JSON.stringify(t,null,2)}async function P(){o="refresh",v();try{const[t,e,a]=await Promise.all([h("status"),h("list-containers"),h("list-images")]);s=t,k=Array.isArray(e.containers)?e.containers:[],y=Array.isArray(a.images)?a.images:[],t.ok===!1&&(b=x(t))}catch(t){b=t instanceof Error?t.message:String(t),g("刷新失败")}finally{o="",v()}}async function d(t,e,a=[],l=!0){o=e,v();try{const n=await h(e,a);b=x(n),g(n.ok===!1?`${t}失败`:`${t}完成`),l&&await P()}catch(n){b=n instanceof Error?n.message:String(n),g(`${t}失败`)}finally{o="",v()}}async function K(){const t=L.trim(),e=A.trim(),a=_.trim(),l=q.trim(),n=D.trim(),i=C.trim()||"bridge";if(!H(t)){g("容器名只能包含字母、数字、点、下划线和短横线");return}if(!N(e)){g("镜像名包含不支持的字符");return}if(!w(a,/^[0-9:/a-z.-]+$/)){g("端口映射格式不合法");return}if(!w(l,/^[A-Za-z_][A-Za-z0-9_=@.,:/+-]*$/)){g("环境变量格式不合法");return}if(!w(n,/^\/[A-Za-z0-9_./:@,+=-]+:\/[A-Za-z0-9_./:@,+=-]+$/)){g("挂载格式不合法");return}await d("创建容器","add-container",[t,e,a,l,n,i])}async function W(){const t=I.trim();if(!N(t)){g("镜像名包含不支持的字符");return}await d("拉取镜像","pull-image",[t])}async function G(t){await d(t?"开启自启":"关闭自启","set-autostart",[t?"on":"off"])}async function Q(){f="diagnostics",await d("运行检查","check",[],!1)}function Y(t){return t.status.toLowerCase().startsWith("up")}function O(){const t=!!s?.running;return`<span class="pill ${t?"ok":"stop"}"><span></span>${t?"Docker 运行中":"Docker 已停止"}</span>`}function $(t,e,a=""){return`<section class="card stat ${a}"><span>${t}</span><strong>${r(e)}</strong></section>`}function u(t,e){return`<div class="detail-item"><span>${t}</span><strong>${r(e||"—")}</strong></div>`}function m(t,e){return`<button class="nav-item ${f===t?"active":""}" data-tab="${t}">${e}</button>`}function V(){return`
    <section class="grid stats-grid">
      ${$("容器总数",s?.containers_total??"—")}
      ${$("运行中",s?.containers_running??"—","green")}
      ${$("已停止",s?.containers_stopped??"—","amber")}
      ${$("镜像",s?.images??(y.length||"—"))}
    </section>

    <section class="panel">
      <div class="section-head">
        <div>
          <p class="eyebrow">Runtime</p>
          <h2>运行时概览</h2>
        </div>
        ${O()}
      </div>
      <div class="detail-grid">
        ${u("Docker 版本",s?.server_version)}
        ${u("Storage Driver",s?.storage_driver)}
        ${u("Cgroup",s?.cgroup_version)}
        ${u("dockerd pid",s?.dockerd_pid)}
        ${u("containerd pid",s?.containerd_pid)}
        ${u("Socket",s?.socket?"ready":"missing")}
        ${u("Base 模块",s?.base_present?"present":"missing")}
        ${u("Data root",s?.data_root)}
        ${u("开机自启",s?.autostart?"已开启":"未开启")}
      </div>
    </section>
  `}function X(){const t=E.trim().toLowerCase();return t?k.filter(e=>[e.name,e.id,e.image,e.status].some(a=>a.toLowerCase().includes(t))):k}function tt(){const t=X();return t.length?t.map(e=>{const a=e.id||e.name,l=Y(e);return`
        <article class="table-row container-row">
          <div class="identity-cell">
            <strong title="${r(e.name||e.id)}">${r(e.name||"(无名称)")}</strong>
            <small title="${r(e.id)}">ID: ${r(e.id)}</small>
          </div>
          <div class="meta-cell">
            <span title="${r(e.image)}">${r(e.image)}</span>
            <small title="${r(e.created)}">创建时间: ${r(e.created)}</small>
          </div>
          <div><span class="badge ${l?"green":"slate"}">${r(e.status)}</span></div>
          <div class="row-actions">
            <button class="small" data-container-action="start" data-target="${r(a)}" ${o||l?"disabled":""}>启动</button>
            <button class="small" data-container-action="stop" data-target="${r(a)}" ${o||!l?"disabled":""}>停止</button>
            <button class="small" data-container-action="restart" data-target="${r(a)}" ${o?"disabled":""}>重启</button>
            <button class="small ghost" data-container-action="logs" data-target="${r(a)}" ${o?"disabled":""}>日志</button>
            <button class="small ghost" data-container-action="inspect" data-target="${r(a)}" ${o?"disabled":""}>Inspect</button>
            <button class="small danger" data-container-action="delete" data-target="${r(a)}" ${o?"disabled":""}>删除</button>
          </div>
        </article>`}).join(""):'<div class="empty">没有匹配的容器。可以用下方表单创建 detached 容器。</div>'}function et(){return`
    <section class="panel">
      <div class="section-head split">
        <div>
          <p class="eyebrow">Containers</p>
          <h2>容器管理</h2>
        </div>
        <input class="search" id="container-search" value="${r(E)}" placeholder="搜索名称、镜像或状态" />
      </div>
      <div class="table-list">${tt()}</div>
    </section>

    <section class="panel create-panel">
      <div>
        <p class="eyebrow">Run detached</p>
        <h2>创建容器</h2>
      </div>
      <div class="form-grid">
        <label>容器名<input id="name" value="${r(L)}" placeholder="demo-nginx" /></label>
        <label>镜像名<input id="image" value="${r(A)}" placeholder="nginx:alpine" /></label>
        <label>网络<input id="network" value="${r(C)}" placeholder="bridge" /></label>
        <label>端口映射<input id="ports" value="${r(_)}" placeholder="8080:80,8443:443" /></label>
        <label>环境变量<input id="envs" value="${r(q)}" placeholder="KEY=value,DEBUG=1" /></label>
        <label>Bind mount<input id="mounts" value="${r(D)}" placeholder="/sdcard/www:/usr/share/nginx/html" /></label>
      </div>
      <button data-action="add" ${o?"disabled":""}>创建容器</button>
    </section>
  `}function at(t){return t.repository==="<none>"||t.tag==="<none>"?t.id:`${t.repository}:${t.tag}`}function nt(){const t=y.length?y.map(e=>`
          <article class="table-row image-row">
            <div>
              <strong>${r(at(e))}</strong>
              <small>${r(e.id)}</small>
            </div>
            <div><span>${r(e.size)}</span><small>${r(e.created)}</small></div>
            <div class="row-actions">
              <button class="small danger" data-remove-image="${r(e.id)}" ${o?"disabled":""}>删除镜像</button>
            </div>
          </article>`).join(""):'<div class="empty">暂无镜像。可以先拉取镜像，或从命令行 import 本地 rootfs。</div>';return`
    <section class="panel create-panel">
      <div>
        <p class="eyebrow">Images</p>
        <h2>拉取镜像</h2>
      </div>
      <label>镜像名<input id="pull-image" value="${r(I)}" placeholder="alpine:latest" /></label>
      <button data-action="pull-image" ${o?"disabled":""}>Pull</button>
    </section>
    <section class="panel">
      <div class="section-head">
        <div>
          <p class="eyebrow">Local images</p>
          <h2>本地镜像</h2>
        </div>
      </div>
      <div class="table-list">${t}</div>
    </section>
  `}function rt(){return`
    <section class="panel actions-panel">
      <div>
        <p class="eyebrow">Diagnostics</p>
        <h2>诊断与日志</h2>
      </div>
      <div class="button-row">
        <button data-action="check" ${o?"disabled":""}>运行 runtime check</button>
        <button data-action="daemon-logs" ${o?"disabled":""}>查看 daemon 日志</button>
        <button data-action="refresh" ${o?"disabled":""}>刷新状态</button>
      </div>
    </section>
    <section class="panel output-panel">
      <div class="section-head">
        <h2>输出</h2>
      </div>
      <pre>${r(b||s?.docker_error||"暂无输出")}</pre>
    </section>
  `}function st(){const t=!!s?.autostart;return`
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
        <button class="switch ${t?"on":""}" data-action="toggle-autostart" ${o?"disabled":""}>
          <span></span>${t?"已开启":"已关闭"}
        </button>
      </div>
      <div class="detail-grid compact">
        ${u("API",S)}
        ${u("Data root",s?.data_root)}
        ${u("Autostart file",s?.autostart_file)}
        ${u("Base 模块",s?.base_present?"present":"missing")}
      </div>
    </section>
  `}function it(){return f==="containers"?et():f==="images"?nt():f==="diagnostics"?rt():f==="settings"?st():V()}function v(){c.innerHTML=`
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
            ${O()}
            <button data-action="refresh" ${o?"disabled":""}>${o==="refresh"?"刷新中…":"刷新"}</button>
            <button data-action="start" ${o||s?.running?"disabled":""}>启动</button>
            <button class="danger" data-action="stop" ${o||!s?.running?"disabled":""}>停止</button>
          </div>
        </header>
        ${it()}
      </section>
    </main>
  `,c.querySelectorAll("[data-tab]").forEach(t=>{t.addEventListener("click",()=>{f=t.dataset.tab,v()})}),c.querySelector('[data-action="refresh"]')?.addEventListener("click",()=>P()),c.querySelector('[data-action="start"]')?.addEventListener("click",()=>d("启动 Docker","start-docker")),c.querySelector('[data-action="stop"]')?.addEventListener("click",()=>d("停止 Docker","stop-docker")),c.querySelector('[data-action="check"]')?.addEventListener("click",()=>Q()),c.querySelector('[data-action="daemon-logs"]')?.addEventListener("click",()=>d("读取日志","daemon-logs",[],!1)),c.querySelector('[data-action="add"]')?.addEventListener("click",()=>K()),c.querySelector('[data-action="pull-image"]')?.addEventListener("click",()=>W()),c.querySelector('[data-action="toggle-autostart"]')?.addEventListener("click",()=>G(!s?.autostart)),c.querySelector("#container-search")?.addEventListener("input",t=>{E=t.target.value,v()}),c.querySelector("#name")?.addEventListener("input",t=>{L=t.target.value}),c.querySelector("#image")?.addEventListener("input",t=>{A=t.target.value}),c.querySelector("#network")?.addEventListener("input",t=>{C=t.target.value}),c.querySelector("#ports")?.addEventListener("input",t=>{_=t.target.value}),c.querySelector("#envs")?.addEventListener("input",t=>{q=t.target.value}),c.querySelector("#mounts")?.addEventListener("input",t=>{D=t.target.value}),c.querySelector("#pull-image")?.addEventListener("input",t=>{I=t.target.value}),c.querySelectorAll("[data-container-action]").forEach(t=>{t.addEventListener("click",()=>{const e=t.dataset.containerAction||"",a=t.dataset.target||"";a&&(e==="logs"&&d("读取容器日志","container-logs",[a],!1),e==="inspect"&&d("Inspect 容器","inspect-container",[a],!1),e==="delete"&&d("删除容器","delete-container",[a]),e==="start"&&d("启动容器","start-container",[a]),e==="stop"&&d("停止容器","stop-container",[a]),e==="restart"&&d("重启容器","restart-container",[a]))})}),c.querySelectorAll("[data-remove-image]").forEach(t=>{t.addEventListener("click",()=>{const e=t.dataset.removeImage||"";e&&d("删除镜像","remove-image",[e])})})}v();J().finally(()=>P());
