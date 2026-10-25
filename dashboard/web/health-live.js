/* Live transport only. The maintained Pulso viewer renders validated reports. */
"use strict";
(()=>{
 const $=id=>document.getElementById(id),tg=window.Telegram?.WebApp;
 let token,busy=false,timer,stopped=false,lastReport,initial=true;
 const initialDeadline=Date.now()+60000,initialRead=()=>initial&&Date.now()<initialDeadline;
 const open=$('open'),status=$('status'),notice=$('report-date').parentElement;
 open.textContent='Actualizar ahora';open.disabled=true;
 // The file-reader handler belongs to exports. Replace only this live view's
 // button, preserving the shared renderer and its explicit report validation.
 const refresh=open.cloneNode(true);open.replaceWith(refresh);
 $('file').remove();$('empty').hidden=true;$('explain').textContent='Sobre estas lecturas';
 notice.querySelector('strong').textContent='Tus compus · actualización automática';
 notice.querySelector('.small').textContent='Consultamos al abrir y cada 30 segundos mientras la app está visible. Cada equipo conserva la fecha de su última lectura.';
 $('help').replaceChildren();const help=document.createElement('p');help.textContent='Sólo se consultan las máquinas autorizadas para tu daimon. Si una no responde, conservamos su lectura anterior con su fecha. Un servicio activo no garantiza que la aplicación responda.';$('help').append(help);
 const sessions=document.createElement('a');sessions.className='button secondary';sessions.href='?view=sessions';sessions.textContent='Sessions';$('help').append(sessions);
 document.querySelector('footer p').textContent='Pulso · consulta privada, sin acciones de reparación.';
 async function api(route,body){const controller=new AbortController(),deadline=setTimeout(()=>controller.abort(),12000);try{const headers=body?{'Content-Type':'application/json'}:{};if(token)headers.Authorization='Bearer '+token;const response=await fetch('api/'+route,{method:body?'POST':'GET',credentials:'same-origin',redirect:'error',headers,body:body?JSON.stringify(body):undefined,signal:controller.signal});if(!response.ok)throw Object.assign(new Error('request_unavailable'),{status:response.status});return await response.json()}finally{clearTimeout(deadline)}}
 function schedule(seconds){clearTimeout(timer);if(!stopped&&(!document.hidden||initialRead()))timer=setTimeout(()=>read(initialRead()),seconds*1000)}
 function clearPrivate(){lastReport=undefined;$('machines').replaceChildren();$('report-date').textContent='Sin lectura autorizada.';}
 function render(value){if(!value.updating)initial=false;if(value.report){lastReport=value.report;document.dispatchEvent(new CustomEvent('pulso:report',{detail:value.report}));$('empty').hidden=true;}else $('report-date').textContent='Tomando la primera lectura…';status.textContent=value.updating?'Actualizando las lecturas…':value.last_attempt_failed?'No pudimos actualizar. La fecha de cada lectura sigue visible.':value.report?'Lecturas actualizadas automáticamente mientras la app está abierta.':'No hay una lectura disponible. Podés volver a intentar.';schedule(value.updating?2:30)}
 async function read(force=false){if(busy||stopped||document.hidden&&!force)return;busy=true;refresh.disabled=true;try{render(await api('computer-health'));$('error').hidden=true;}catch(error){if(error.status===401||error.status===403){stopped=true;clearPrivate();status.textContent='Abrí Pulso desde tu bot en Mis Apps para volver a identificarte.';}else if(error.status===503){stopped=true;clearPrivate();status.textContent='El acceso a estas compus requiere revisión del servicio. No se muestran lecturas anteriores sin autorización.';}else{status.textContent=lastReport?'Se interrumpió la conexión. Las lecturas conservan su fecha.':'No pudimos conectar. Podés volver a intentar.';schedule(15)}}finally{busy=false;refresh.disabled=stopped}}
 refresh.addEventListener('click',()=>read(true));
 document.addEventListener('visibilitychange',()=>{clearTimeout(timer);if(!document.hidden)read()});
 tg?.onEvent?.('activated',()=>read());tg?.onEvent?.('deactivated',()=>clearTimeout(timer));
 function theme(){const bg=getComputedStyle(document.body).backgroundColor;for(const method of ['setHeaderColor','setBackgroundColor','setBottomBarColor'])try{tg?.[method]?.(bg)}catch{}}
 window.addEventListener('home-style:resolved',theme);theme();tg?.ready?.();
 (async()=>{status.textContent='Conectando con tus compus…';try{if(tg?.initData){const session=await api('session',{init_data:tg.initData});if(!/^[A-Za-z0-9_-]{43}$/.test(session.session_token))throw Error('invalid_session');token=session.session_token;}refresh.disabled=false;await read(true)}catch{clearPrivate();status.textContent='No pudimos identificarte. Cerrá Pulso y abrila desde tu bot en Mis Apps.'}})();
})();
