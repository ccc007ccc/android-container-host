(function(){const t=document.createElement("link").relList;if(t&&t.supports&&t.supports("modulepreload"))return;for(const r of document.querySelectorAll('link[rel="modulepreload"]'))a(r);new MutationObserver(r=>{for(const n of r)if(n.type==="childList")for(const d of n.addedNodes)d.tagName==="LINK"&&d.rel==="modulepreload"&&a(d)}).observe(document,{childList:!0,subtree:!0});function s(r){const n={};return r.integrity&&(n.integrity=r.integrity),r.referrerPolicy&&(n.referrerPolicy=r.referrerPolicy),r.crossOrigin==="use-credentials"?n.credentials="include":r.crossOrigin==="anonymous"?n.credentials="omit":n.credentials="same-origin",n}function a(r){if(r.ep)return;r.ep=!0;const n=s(r);fetch(r.href,n)}})();let w=0;function E(e){return`${e}_callback_${Date.now()}_${w++}`}function L(e,t){return typeof t>"u"&&(t={}),new Promise((s,a)=>{const r=E("exec");window[r]=(d,k,S)=>{s({errno:d,stdout:k,stderr:S}),n(r)};function n(d){delete window[d]}try{ksu.exec(e,JSON.stringify(t),r)}catch(d){a(d),n(r)}})}function _(e){ksu.enableEdgeToEdge(e)}function q(e){ksu.toast(e)}const D="/data/adb/modules/achost-runtime/achost/bin/achost-webui-api.sh",u=document.querySelector("#app");let o=null,y=[],g="",c="",$="",v="";try{_(!0)}catch{}function l(e){return String(e??"").replace(/&/g,"&amp;").replace(/</g,"&lt;").replace(/>/g,"&gt;").replace(/"/g,"&quot;").replace(/'/g,"&#39;")}function A(e){return`'${e.replace(/'/g,"'\\''")}'`}function x(e){return/^[A-Za-z0-9_.-]+$/.test(e)}function N(e){return/^[A-Za-z0-9_./:@-]+$/.test(e)}function p(e){try{q(e)}catch{}}async function h(e,t=[]){const s=[D,e,...t.map(A)].join(" "),a=await L(s),r=a.stdout.trim()||a.stderr.trim();if(!r)return{ok:!1,error:`命令没有输出，errno=${a.errno}`};try{const n=JSON.parse(r);return a.errno!==0&&n.ok!==!1&&(n.ok=!1,n.error=`errno=${a.errno}`),n}catch{return{ok:!1,error:r}}}async function b(){c="refresh",f();try{const[e,t]=await Promise.all([h("status"),h("list-containers")]);o=e,y=Array.isArray(t.containers)?t.containers:[]}catch(e){g=e instanceof Error?e.message:String(e),p("刷新失败")}finally{c="",f()}}async function m(e,t,s=[]){c=t,f();try{const a=await h(t,s);g=String(a.output||a.error||"完成"),p(a.ok===!1?`${e}失败`:`${e}完成`),await b()}catch(a){g=a instanceof Error?a.message:String(a),p(`${e}失败`)}finally{c="",f()}}async function O(){const e=$.trim(),t=v.trim();if(!x(e)){p("容器名只能包含字母、数字、点、下划线和短横线");return}if(!N(t)){p("镜像名包含不支持的字符");return}await m("新增容器","add-container",[e,t])}async function C(){c="check",f();try{const e=await h("check");g=String(e.output||e.error||"检查完成"),p(e.ok===!1?"检查发现问题":"检查通过")}catch(e){g=e instanceof Error?e.message:String(e),p("检查失败")}finally{c="",f()}}function i(e,t,s=""){return`<section class="card stat ${s}"><span>${e}</span><strong>${l(t)}</strong></section>`}function P(){const e=!!o?.running;return`<span class="pill ${e?"ok":"stop"}"><span></span>${e?"Docker 运行中":"Docker 已停止"}</span>`}function I(){return y.length?y.map(e=>`
        <article class="container-row">
          <div>
            <strong>${l(e.name||e.id.slice(0,12))}</strong>
            <small>${l(e.id.slice(0,12))}</small>
          </div>
          <div>
            <span>${l(e.image)}</span>
            <small>${l(e.created)}</small>
          </div>
          <div><span class="status-text">${l(e.status)}</span></div>
          <button class="danger" data-delete="${l(e.name||e.id)}">删除</button>
        </article>`).join(""):'<div class="empty">暂无容器。可以在下方输入名称和镜像创建一个 detached bridge 容器。</div>'}function f(){const e=!!o?.running;u.innerHTML=`
    <main class="shell">
      <header class="hero">
        <div>
          <p class="eyebrow">KernelSU Module WebUI</p>
          <h1>ACHost Docker</h1>
          <p class="subtle">查看 Docker 状态、容器数量，并执行受限的新增、删除和检查操作。</p>
        </div>
        ${P()}
      </header>

      <section class="actions panel">
        <button data-action="refresh" ${c?"disabled":""}>${c==="refresh"?"刷新中…":"刷新"}</button>
        <button data-action="start" ${c||e?"disabled":""}>启动 Docker</button>
        <button data-action="stop" ${c||!e?"disabled":""}>停止 Docker</button>
        <button data-action="check" ${c?"disabled":""}>运行检查</button>
      </section>

      <section class="grid stats-grid">
        ${i("容器总数",o?.containers_total??"—")}
        ${i("运行中",o?.containers_running??"—","green")}
        ${i("已停止",o?.containers_stopped??"—","amber")}
        ${i("镜像",o?.images??"—")}
      </section>

      <section class="grid detail-grid">
        ${i("dockerd pid",o?.dockerd_pid||"—")}
        ${i("containerd pid",o?.containerd_pid||"—")}
        ${i("socket",o?.socket?"ready":"missing",o?.socket?"green":"amber")}
        ${i("cgroup",o?.cgroup_version||"—")}
        ${i("Docker",o?.server_version||"—")}
        ${i("Storage",o?.storage_driver||"—")}
      </section>

      <section class="panel">
        <div class="section-head">
          <div>
            <p class="eyebrow">Containers</p>
            <h2>容器列表</h2>
          </div>
        </div>
        <div class="container-list">${I()}</div>
      </section>

      <section class="panel add-panel">
        <div>
          <p class="eyebrow">Run detached</p>
          <h2>新增容器</h2>
        </div>
        <label>容器名<input id="name" value="${l($)}" placeholder="demo-nginx" /></label>
        <label>镜像名<input id="image" value="${l(v)}" placeholder="nginx:alpine" /></label>
        <button data-action="add" ${c?"disabled":""}>创建</button>
      </section>

      <section class="panel output-panel">
        <div class="section-head">
          <h2>检查输出</h2>
        </div>
        <pre>${l(g||o?.docker_error||"暂无输出")}</pre>
      </section>
    </main>
  `,u.querySelector('[data-action="refresh"]')?.addEventListener("click",()=>b()),u.querySelector('[data-action="start"]')?.addEventListener("click",()=>m("启动 Docker","start-docker")),u.querySelector('[data-action="stop"]')?.addEventListener("click",()=>m("停止 Docker","stop-docker")),u.querySelector('[data-action="check"]')?.addEventListener("click",()=>C()),u.querySelector('[data-action="add"]')?.addEventListener("click",()=>O()),u.querySelector("#name")?.addEventListener("input",t=>{$=t.target.value}),u.querySelector("#image")?.addEventListener("input",t=>{v=t.target.value}),u.querySelectorAll("[data-delete]").forEach(t=>{t.addEventListener("click",()=>{const s=t.dataset.delete||"";s&&m("删除容器","delete-container",[s])})})}f();b();
