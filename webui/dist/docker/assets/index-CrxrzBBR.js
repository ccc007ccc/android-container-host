(function(){const e=document.createElement("link").relList;if(e&&e.supports&&e.supports("modulepreload"))return;for(const o of document.querySelectorAll('link[rel="modulepreload"]'))s(o);new MutationObserver(o=>{for(const i of o)if(i.type==="childList")for(const c of i.addedNodes)c.tagName==="LINK"&&c.rel==="modulepreload"&&s(c)}).observe(document,{childList:!0,subtree:!0});function a(o){const i={};return o.integrity&&(i.integrity=o.integrity),o.referrerPolicy&&(i.referrerPolicy=o.referrerPolicy),o.crossOrigin==="use-credentials"?i.credentials="include":o.crossOrigin==="anonymous"?i.credentials="omit":i.credentials="same-origin",i}function s(o){if(o.ep)return;o.ep=!0;const i=a(o);fetch(o.href,i)}})();let R=0;function q(t){return`${t}_callback_${Date.now()}_${R++}`}function F(t,e){return typeof e>"u"&&(e={}),new Promise((a,s)=>{const o=q("exec");window[o]=(c,b,h)=>{a({errno:c,stdout:b,stderr:h}),i(o)};function i(c){delete window[c]}try{ksu.exec(t,JSON.stringify(e),o)}catch(c){s(c),i(o)}})}function U(t){ksu.enableEdgeToEdge(t)}function J(t){ksu.toast(t)}const Z="/data/adb/modules/achost-docker/achost/bin/achost-webui-api.sh",k=document.querySelector("#app"),n={apiPath:Z,moduleId:"achost-docker",activeTab:"dashboard",statusData:null,containers:[],images:[],output:"",busy:"",ui:{sheet:{kind:"none"},expanded:{}},forms:{containerSearch:"",dockerName:"",dockerImage:"",dockerPorts:"",dockerEnv:"",dockerMounts:"",dockerNetwork:"bridge",imageToPull:""}};try{U(!0)}catch{}function r(t){return String(t??"").replace(/&/g,"&amp;").replace(/</g,"&lt;").replace(/>/g,"&gt;").replace(/"/g,"&quot;").replace(/'/g,"&#39;")}function K(t){return`'${t.replace(/'/g,"'\\''")}'`}function f(t){try{J(t)}catch{}}function W(t){return/^[A-Za-z0-9_.-]+$/.test(t)&&t!=="."&&t!==".."&&!t.includes("..")}function P(t){return/^[A-Za-z0-9_./:@-]+$/.test(t)}function D(t,e){return!t.trim()||t.split(",").every(a=>e.test(a.trim()))}function y(t){return`achost-webui:docker:${n.moduleId}:${t}`}function I(t){try{return localStorage.getItem(t)}catch{return null}}function C(t,e){try{localStorage.setItem(t,e)}catch{}}function E(){return[{tab:"dashboard",label:"概览"},{tab:"containers",label:"容器"},{tab:"images",label:"镜像"},{tab:"diagnostics",label:"诊断"},{tab:"settings",label:"设置"}]}function N(t){return E().some(e=>e.tab===t)}function G(t){typeof t.api=="string"&&t.api.startsWith("/data/adb/modules/")&&(n.apiPath=t.api),typeof t.moduleId=="string"&&t.moduleId&&(n.moduleId=t.moduleId)}function Q(){const e=document.querySelector('meta[name="achost-webui-config"]')?.content||"";if(e)try{G(JSON.parse(e))}catch{}}function V(){const t=I(y("active-tab"));n.activeTab=t&&N(t)?t:"dashboard";const e=I(y("expanded"));if(e)try{const a=JSON.parse(e);n.ui.expanded=Object.fromEntries(Object.entries(a).filter(([,s])=>typeof s=="boolean"))}catch{n.ui.expanded={}}}function O(){C(y("active-tab"),n.activeTab)}function Y(){C(y("expanded"),JSON.stringify(n.ui.expanded))}async function $(t,e=[]){const a=[n.apiPath,t,...e].map(K).join(" "),s=await F(a);return X(s)}function X(t){const e=t.stdout.trim()||t.stderr.trim();if(!e)return{ok:!1,error:`命令没有输出，errno=${t.errno}`};try{const a=JSON.parse(e);return t.errno!==0&&a.ok!==!1&&(a.ok=!1,a.error=`errno=${t.errno}`),a}catch{return{ok:!1,error:e}}}function v(t){return typeof t.output=="string"?t.output:typeof t.error=="string"?t.error:JSON.stringify(t,null,2)}function tt(t){return Array.isArray(t)?t.map(e=>{const a=e;return{id:String(a.id??""),name:String(a.name??""),image:String(a.image??""),status:String(a.status??""),created:String(a.created??"")}}).filter(e=>e.id||e.name):[]}function et(t){return Array.isArray(t)?t.map(e=>{const a=e;return{repository:String(a.repository??""),tag:String(a.tag??""),id:String(a.id??""),size:String(a.size??""),created:String(a.created??"")}}).filter(e=>e.id||e.repository):[]}async function x(){const[t,e,a]=await Promise.all([$("status"),$("list-containers"),$("list-images")]);n.statusData=t,n.containers=tt(e.containers),n.images=et(a.images),t.ok===!1?n.output=v(t):e.ok===!1?n.output=v(e):a.ok===!1&&(n.output=v(a))}async function L(){n.busy="refresh",l();try{await x()}catch(t){n.output=t instanceof Error?t.message:String(t),f("刷新失败")}finally{n.busy="",l()}}async function d(t,e,a=[],s=!0,o={}){n.busy=e,l();try{const i=await $(e,a),c=i.ok===!1;n.output=v(i),f(c?`${t}失败`:`${t}完成`),s&&await x(),c||o.showOutput||!s?n.ui.sheet={kind:"output",title:c?`${t}失败`:t}:n.ui.sheet.kind!=="none"&&(n.ui.sheet={kind:"none"})}catch(i){n.output=i instanceof Error?i.message:String(i),n.ui.sheet={kind:"output",title:`${t}失败`},f(`${t}失败`)}finally{n.busy="",l()}}async function nt(){const t=n.forms.dockerName.trim(),e=n.forms.dockerImage.trim(),a=n.forms.dockerPorts.trim(),s=n.forms.dockerEnv.trim(),o=n.forms.dockerMounts.trim(),i=n.forms.dockerNetwork.trim()||"bridge";if(!W(t))return f("容器名只能包含字母、数字、点、下划线和短横线");if(!P(e))return f("镜像名包含不支持的字符");if(!D(a,/^[0-9:/a-z.-]+$/))return f("端口映射格式不合法");if(!D(s,/^[A-Za-z_][A-Za-z0-9_=@.,:/+-]*$/))return f("环境变量格式不合法");if(!D(o,/^\/[A-Za-z0-9_./:@,+=-]+:\/[A-Za-z0-9_./:@,+=-]+$/))return f("挂载格式不合法");await d("创建容器","add-container",[t,e,a,s,o,i])}async function at(){const t=n.forms.imageToPull.trim();if(!P(t))return f("镜像名包含不支持的字符");await d("拉取镜像","pull-image",[t])}async function st(t){await d(t?"开启自启":"关闭自启","set-autostart",[t?"on":"off"])}async function rt(){n.activeTab="diagnostics",O(),await d("运行检查","check",[],!1,{showOutput:!0})}function z(t){return t.status.toLowerCase().startsWith("up")}function w(){const t=!!n.statusData?.running;return`<span class="pill ${t?"ok":"stop"}"><span></span>${t?"Docker 运行":"Docker 停止"}</span>`}function ot(t,e,a=""){return`<article class="metric-card ${a}"><span>${r(t)}</span><strong>${r(e)}</strong></article>`}function it(t){return`<section class="metric-strip">${t.map(([e,a,s])=>ot(e,a,s)).join("")}</section>`}function ct(t,e){return`<div class="detail-item"><span>${r(t)}</span><strong>${r(e||"—")}</strong></div>`}function S(t){return`<div class="detail-grid">${t.map(([e,a])=>ct(e,a)).join("")}</div>`}function M(t,e=!1){return n.ui.expanded[t]??e}function g(t,e,a,s,o=!1,i=""){const c=M(t,o);return`
    <section class="panel accordion ${c?"open":""}">
      <button type="button" class="accordion-trigger" data-accordion="${r(t)}" aria-expanded="${c?"true":"false"}">
        <span>
          <small>${r(a)}</small>
          <strong>${r(e)}</strong>
        </span>
        <span class="accordion-side">${i}<span class="chevron">${c?"收起":"展开"}</span></span>
      </button>
      ${c?`<div class="accordion-body">${s}</div>`:""}
    </section>`}function ut(t){return`
    <section class="page-intro">
      <div>
        <p class="eyebrow">KernelSU Module WebUI</p>
        <h1>${r(t)}</h1>
      </div>
      <div class="top-actions">${vt()}</div>
    </section>`}function j(t,e){return`
    <section class="panel command-panel">
      <div>
        <p class="eyebrow">Control</p>
        <h2>${r(t)}</h2>
      </div>
      <div class="button-row">${e}</div>
    </section>`}function u(t,e,a="",s=!1){return`<button type="button" class="${r(a)}" data-action="${r(e)}" ${s?"disabled":""}>${r(t)}</button>`}function dt(){return`
    ${it([["容器",n.statusData?.containers_total??"—"],["运行",n.statusData?.containers_running??"—","success"],["停止",n.statusData?.containers_stopped??"—","warning"],["镜像",n.statusData?.images??(n.images.length||"—")]])}
    ${j("Docker 控制台",`${u("刷新","refresh","ghost",!!n.busy)}${u("启动 Docker","start","",!!(n.busy||n.statusData?.running))}${u("停止 Docker","stop","danger",!!(n.busy||!n.statusData?.running))}${u("运行检查","check","ghost",!!n.busy)}`)}
    ${g("docker-runtime","运行时详情","Runtime",S([["Docker 版本",n.statusData?.server_version],["Storage Driver",n.statusData?.storage_driver],["Cgroup",n.statusData?.cgroup_version],["dockerd pid",n.statusData?.dockerd_pid],["containerd pid",n.statusData?.containerd_pid],["Socket",n.statusData?.socket?"ready":"missing"],["Base 模块",n.statusData?.base_present?"present":"missing"],["Data root",n.statusData?.data_root],["开机自启",n.statusData?.autostart?"已开启":"未开启"]]),!1,w())}
    ${m("docker-output")}
  `}function lt(){const t=n.forms.containerSearch.trim().toLowerCase();return t?n.containers.filter(e=>[e.name,e.id,e.image,e.status].some(a=>a.toLowerCase().includes(t))):n.containers}function ft(t,e,a=""){return`
    <section class="panel list-tools">
      <div>
        <p class="eyebrow">Inventory</p>
        <h2>${r(t)}</h2>
      </div>
      <div class="toolbar-row">
        <input class="search" id="container-search" value="${r(n.forms.containerSearch)}" placeholder="${r(e)}" autocomplete="off" />
        ${a}
      </div>
    </section>`}function pt(){const t=lt();return t.length?t.map(e=>{const a=e.id||e.name,s=z(e),o=s?"stop":"start";return`
        <article class="entity-card">
          <div class="entity-main">
            <strong title="${r(e.name||e.id)}">${r(e.name||"(无名称)")}</strong>
            <small title="${r(e.id)}">${r(e.id)}</small>
          </div>
          <div class="entity-meta">
            <span title="${r(e.image)}">${r(e.image)}</span>
            <small title="${r(e.created)}">创建: ${r(e.created||"—")}</small>
          </div>
          <div class="entity-status"><span class="badge ${s?"green":"slate"}">${r(e.status)}</span></div>
          <div class="entity-actions">
            <button type="button" class="small" data-container-action="${o}" data-target="${r(a)}" ${n.busy?"disabled":""}>${s?"停止":"启动"}</button>
            <button type="button" class="small ghost" data-container-action="more" data-target="${r(a)}" ${n.busy?"disabled":""}>更多</button>
          </div>
        </article>`}).join(""):'<div class="empty">没有匹配的容器。</div>'}function gt(){return`
    ${ft("容器管理","搜索名称、镜像或状态",`<button type="button" data-sheet="docker-run" ${n.busy?"disabled":""}>创建容器</button>`)}
    <section class="entity-list">${pt()}</section>
    ${m("container-output")}
  `}function _(t){return t.repository==="<none>"||t.tag==="<none>"?t.id:`${t.repository}:${t.tag}`}function mt(){const t=n.images.length?n.images.map(e=>`
            <article class="entity-card image-card">
              <div class="entity-main">
                <strong>${r(_(e))}</strong>
                <small>${r(e.id)}</small>
              </div>
              <div class="entity-meta"><span>${r(e.size)}</span><small>${r(e.created)}</small></div>
              <div class="entity-actions">
                <button type="button" class="small ghost" data-image-action="more" data-target="${r(e.id)}" ${n.busy?"disabled":""}>更多</button>
              </div>
            </article>`).join(""):'<div class="empty">暂无镜像。可以先拉取镜像，或从命令行导入本地镜像。</div>';return`
    <section class="panel list-tools">
      <div>
        <p class="eyebrow">Images</p>
        <h2>本地镜像</h2>
      </div>
      <button type="button" data-sheet="image-pull" ${n.busy?"disabled":""}>拉取镜像</button>
    </section>
    <section class="entity-list">${t}</section>
    ${m("image-output")}
  `}function bt(){return`
    ${j("诊断与日志",`${u("运行 runtime check","check","",!!n.busy)}${u("查看 daemon 日志","daemon-logs","ghost",!!n.busy)}${u("刷新状态","refresh","ghost",!!n.busy)}`)}
    ${g("docker-compat","非常规环境状态","Android compatibility",S([["Runtime mode",n.statusData?.runtime_mode],["Cgroup mode",n.statusData?.configured_cgroup_mode||n.statusData?.cgroup_version],["Host cgroup",n.statusData?.cgroup_mount],["DNS servers",n.statusData?.dns_servers||n.statusData?.resolv_nameservers],["resolv.conf",n.statusData?.resolv_conf],["resolv nameservers",n.statusData?.resolv_nameservers],["Bridge",n.statusData?.bridge],["Bridge subnet",n.statusData?.bridge_subnet],["Bridge route",n.statusData?.bridge_route],["Return policy",n.statusData?.return_policy_rule],["Source policy",n.statusData?.source_policy_rule],["Uplink",n.statusData?.uplink]]),!1,`<span class="badge ${n.statusData?.route_status==="ok"?"green":"slate"}">路由 ${r(n.statusData?.route_status||"unknown")}</span>`)}
    ${m("diagnostics-output",n.statusData?.docker_error)}
  `}function ht(){const t=!!n.statusData?.autostart;return`
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
        <button type="button" class="switch ${t?"on":""}" data-action="toggle-autostart" ${n.busy?"disabled":""}>
          <span></span>${t?"已开启":"已关闭"}
        </button>
      </div>
    </section>
    ${g("docker-settings-paths","路径与 API","Advanced",S([["API",n.apiPath],["Data root",n.statusData?.data_root],["Autostart file",n.statusData?.autostart_file],["Base 模块",n.statusData?.base_present?"present":"missing"]]),!1)}
    ${m("settings-output")}
  `}function m(t,e){const a=n.output||e||"";return a?g(t,"最近输出","Output",`<pre class="output-pre">${r(a)}</pre><div class="button-row compact"><button type="button" class="ghost" data-action="output">打开输出面板</button></div>`,!1):""}function $t(){return n.activeTab==="containers"?gt():n.activeTab==="images"?mt():n.activeTab==="diagnostics"?bt():n.activeTab==="settings"?ht():dt()}function B(t){return`<nav class="${r(t)}" aria-label="主导航">${E().map(e=>`
        <button type="button" class="nav-item ${n.activeTab===e.tab?"active":""}" data-tab="${e.tab}" ${n.activeTab===e.tab?'aria-current="page"':""}>
          <span>${r(e.label)}</span>
        </button>`).join("")}</nav>`}function vt(){const t=n.output?u("输出","output","ghost",!1):"";return`${w()}${u(n.busy==="refresh"?"刷新中…":"刷新","refresh","ghost",!!n.busy)}${u("启动","start","",!!(n.busy||n.statusData?.running))}${u("停止","stop","danger",!!(n.busy||!n.statusData?.running))}${t}`}function T(){return`
    <div class="brand">
      <span class="logo">AC</span>
      <div>
        <strong>ACHost</strong>
        <small>Docker Panel</small>
      </div>
    </div>`}function yt(){return`
    <main class="app-shell docker-shell">
      <aside class="side-rail">
        ${T()}
        ${B("side-nav")}
      </aside>
      <section class="workspace">
        <header class="mobile-header">
          ${T()}
          ${w()}
        </header>
        ${ut("Docker 管理面板")}
        <div class="page-stack">${$t()}</div>
      </section>
      ${B("bottom-nav")}
      ${kt()}
    </main>`}function kt(){if(n.ui.sheet.kind==="none")return"";let t="",e="",a="";const s=n.ui.sheet;if(s.kind==="docker-run")t="创建 Docker 容器",e=Dt(),a=`${u("创建容器","add","",!!n.busy)}${u("取消","close-sheet","ghost",!1)}`;else if(s.kind==="image-pull")t="拉取镜像",e=`<label>镜像名<input id="pull-image" value="${r(n.forms.imageToPull)}" placeholder="alpine:latest" autofocus /></label>`,a=`${u("Pull","pull-image","",!!n.busy)}${u("取消","close-sheet","ghost",!1)}`;else if(s.kind==="container-actions"){const o=_t(s.target);t=o?.name||s.target,e=wt(s.target,o)}else if(s.kind==="image-actions"){const o=At(s.target);t=o?_(o):s.target,e=St(s.target,o)}else s.kind==="output"?(t=s.title,e=`<pre class="output-pre sheet-output">${r(n.output||"暂无输出")}</pre>`,a=`${u("关闭","close-sheet","ghost",!1)}`):s.kind==="confirm"&&(t=s.title,e=`<p class="confirm-copy">${r(s.message)}</p>`,a=`${u(s.confirmLabel,"confirm-run","danger",!!n.busy)}${u("取消","close-sheet","ghost",!1)}`);return`
    <div class="sheet-layer" data-backdrop="sheet">
      <section class="sheet" role="dialog" aria-modal="true" aria-labelledby="sheet-title">
        <div class="sheet-handle"></div>
        <header class="sheet-head">
          <div>
            <p class="eyebrow">Action sheet</p>
            <h2 id="sheet-title">${r(t)}</h2>
          </div>
          <button type="button" class="icon-button ghost" data-action="close-sheet" aria-label="关闭">关闭</button>
        </header>
        <div class="sheet-body">${e}</div>
        ${a?`<footer class="sheet-footer">${a}</footer>`:""}
      </section>
    </div>`}function Dt(){return`
    <div class="form-grid">
      <label>容器名<input id="name" value="${r(n.forms.dockerName)}" placeholder="demo-nginx" autofocus /></label>
      <label>镜像名<input id="image" value="${r(n.forms.dockerImage)}" placeholder="nginx:alpine" /></label>
      <label>网络<input id="network" value="${r(n.forms.dockerNetwork)}" placeholder="bridge" /></label>
    </div>
    ${g("docker-run-advanced","端口、环境变量和挂载","Advanced",`<div class="form-grid"><label>端口映射<input id="ports" value="${r(n.forms.dockerPorts)}" placeholder="8080:80,8443:443" /></label><label>环境变量<input id="envs" value="${r(n.forms.dockerEnv)}" placeholder="KEY=value,DEBUG=1" /></label><label>Bind mount<input id="mounts" value="${r(n.forms.dockerMounts)}" placeholder="/sdcard/www:/usr/share/nginx/html" /></label></div>`,!1)}`}function wt(t,e){const a=e?z(e):!1;return`
    <div class="sheet-summary">
      <span class="badge ${a?"green":"slate"}">${r(e?.status||"unknown")}</span>
      <small>${r(e?.image||t)}</small>
    </div>
    <div class="action-grid">
      <button type="button" data-container-action="start" data-target="${r(t)}" ${n.busy||a?"disabled":""}>启动</button>
      <button type="button" data-container-action="stop" data-target="${r(t)}" ${n.busy||!a?"disabled":""}>停止</button>
      <button type="button" class="ghost" data-container-action="restart" data-target="${r(t)}" ${n.busy?"disabled":""}>重启</button>
      <button type="button" class="ghost" data-container-action="logs" data-target="${r(t)}" ${n.busy?"disabled":""}>日志</button>
      <button type="button" class="ghost" data-container-action="inspect" data-target="${r(t)}" ${n.busy?"disabled":""}>Inspect</button>
      <button type="button" class="danger" data-container-action="delete" data-target="${r(t)}" ${n.busy?"disabled":""}>删除</button>
    </div>`}function St(t,e){return`
    <div class="sheet-summary">
      <span class="badge slate">${r(e?.size||"image")}</span>
      <small>${r(e?.created||t)}</small>
    </div>
    <div class="action-grid">
      <button type="button" class="danger" data-image-action="remove" data-target="${r(t)}" ${n.busy?"disabled":""}>删除镜像</button>
    </div>`}function _t(t){return n.containers.find(e=>e.id===t||e.name===t)}function At(t){return n.images.find(e=>e.id===t||_(e)===t)}function It(){const t=document.activeElement;return!(t instanceof HTMLInputElement)||!t.id?null:{id:t.id,selectionStart:t.selectionStart,selectionEnd:t.selectionEnd}}function Bt(t,e=!1){queueMicrotask(()=>{if(t){const a=document.getElementById(t.id);if(a instanceof HTMLInputElement){a.focus();try{t.selectionStart!==null&&t.selectionEnd!==null&&a.setSelectionRange(t.selectionStart,t.selectionEnd)}catch{}return}}e&&n.ui.sheet.kind!=="none"&&k.querySelector(".sheet [autofocus], .sheet input, .sheet button")?.focus()})}function l(t=null,e=!1){k.innerHTML=yt(),Bt(t,e)}function Tt(){l(It())}function p(t){n.ui.sheet=t,l(null,!0)}function A(){n.ui.sheet={kind:"none"},l()}function H(t,e,a,s){p({kind:"confirm",title:t,message:e,confirmLabel:a,intent:s})}function Pt(t){return N(t)?(n.activeTab=t,O(),l(),!0):!1}function Ct(t){const e=t.target;if(!(e instanceof HTMLInputElement))return;const a=e.value;if(e.id==="container-search"){n.forms.containerSearch=a,Tt();return}e.id==="name"&&(n.forms.dockerName=a),e.id==="image"&&(n.forms.dockerImage=a),e.id==="network"&&(n.forms.dockerNetwork=a),e.id==="ports"&&(n.forms.dockerPorts=a),e.id==="envs"&&(n.forms.dockerEnv=a),e.id==="mounts"&&(n.forms.dockerMounts=a),e.id==="pull-image"&&(n.forms.imageToPull=a)}async function Et(t){if(t.disabled)return;const a=t.dataset.action||"";if(a==="close-sheet")return A();if(a==="output")return p({kind:"output",title:"输出"});if(a==="refresh")return L();if(a==="start")return d("启动 Docker","start-docker");if(a==="stop")return d("停止 Docker","stop-docker");if(a==="check")return rt();if(a==="daemon-logs")return d("读取日志","daemon-logs",[],!1,{showOutput:!0});if(a==="add")return nt();if(a==="pull-image")return at();if(a==="toggle-autostart")return st(!n.statusData?.autostart);if(a==="confirm-run")return xt()}async function Nt(t){if(t.disabled)return;const a=t.dataset.containerAction||"",s=t.dataset.target||"";if(s){if(a==="more")return p({kind:"container-actions",target:s});if(a==="logs")return d("读取容器日志","container-logs",[s],!1,{showOutput:!0});if(a==="inspect")return d("Inspect 容器","inspect-container",[s],!1,{showOutput:!0});if(a==="delete")return H("删除 Docker 容器",`确认删除 ${s}？该操作不可撤销。`,"删除容器",{kind:"delete-container",target:s});if(a==="start")return d("启动容器","start-container",[s]);if(a==="stop")return d("停止容器","stop-container",[s]);if(a==="restart")return d("重启容器","restart-container",[s])}}async function Ot(t){if(t.disabled)return;const a=t.dataset.imageAction||"",s=t.dataset.target||"";if(s){if(a==="more")return p({kind:"image-actions",target:s});if(a==="remove")return H("删除镜像",`确认删除镜像 ${s}？`,"删除镜像",{kind:"remove-image",target:s})}}async function xt(){const t=n.ui.sheet;if(t.kind==="confirm"){if(t.intent.kind==="delete-container")return d("删除容器","delete-container",[t.intent.target]);if(t.intent.kind==="remove-image")return d("删除镜像","remove-image",[t.intent.target])}}function Lt(t){const e=t.dataset.sheet||"";if(e==="docker-run")return p({kind:"docker-run"});if(e==="image-pull")return p({kind:"image-pull"})}async function zt(t){const e=t.target;if(!(e instanceof HTMLElement))return;if(e.dataset.backdrop==="sheet")return A();const a=e.closest("[data-tab]");if(a){t.preventDefault(),Pt(a.dataset.tab||"");return}const s=e.closest("[data-accordion]");if(s){const h=s.dataset.accordion||"";n.ui.expanded[h]=!M(h),Y(),l();return}const o=e.closest("[data-sheet]");if(o){Lt(o);return}const i=e.closest("[data-action]");if(i){await Et(i);return}const c=e.closest("[data-container-action]");if(c){await Nt(c);return}const b=e.closest("[data-image-action]");b&&await Ot(b)}function Mt(t){t.key==="Escape"&&n.ui.sheet.kind!=="none"&&(t.preventDefault(),A())}k.addEventListener("click",t=>{zt(t)});k.addEventListener("input",Ct);document.addEventListener("keydown",Mt);Q();V();l();L();
