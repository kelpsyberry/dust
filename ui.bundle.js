(()=>{"use strict";(()=>{const t={none:0,"eeprom-4k":1,"eeprom-fram-64k":2,"eeprom-fram-512k":3,"eeprom-fram-1m":4,"flash-2m":5,"flash-4m":6,"flash-8m":7,"nand-64m":8,"nand-128m":9,"nand-256m":10};class e{constructor(t,e,s){this.inputElement=t,this.loadCallback=e,this.storageKey=s,t.addEventListener("change",(()=>{const e=t.files?t.files[0]:null;e&&this.loadFromInput(e)}))}get enabled(){return!this.inputElement.disabled}set enabled(t){this.inputElement.disabled=!t}load(t,e){this.loadCallback(t,e)}unload(){}loadFromInput(t){const e=new FileReader;e.onload=()=>{const s=e.result;this.storageKey?(e.onload=()=>{this.load(t.name,s),this.storeDataURLToStorage(t.name,e.result)},e.readAsDataURL(t)):this.load(t.name,s)},e.readAsArrayBuffer(t)}loadFromStorage(t){if(null!=t||(t=this.storageKey),!t)return;const e=localStorage[t];if(e){const t=e.split(",");if(!t[2])return;const s=atob(t[2]),i=new Uint8Array(s.length);for(let t=s.length;t--;)i[t]=s.charCodeAt(t);this.load(t[0],i.buffer)}}storeDataURLToStorage(t,e,s){null!=s||(s=this.storageKey),s&&(localStorage[s]=t+","+e)}storeToStorage(t,e,s){if(null!=s||(s=this.storageKey),!s)return;let i="";const a=new Uint8Array(e);for(let t=0;t<a.length;t++)i+=String.fromCharCode(a[t]);this.storeDataURLToStorage(t,"data:application/octet-stream;base64,"+btoa(i),s)}}class s extends e{constructor(t,e,s){super(t,e,s),this.labelElement=this.inputElement.nextElementSibling,this.fileNameElement=this.labelElement.getElementsByClassName("file-name")[0],this.loadIndicatorUse=this.labelElement.querySelector(".load-indicator > use")}load(t,e){super.load(t,e),this.fileNameElement.textContent=t,this.loadIndicatorUse.setAttributeNS("http://www.w3.org/1999/xlink","xlink:href","file-check.svg#icon")}unload(){super.unload(),this.fileNameElement.textContent="",this.loadIndicatorUse.setAttributeNS("http://www.w3.org/1999/xlink","xlink:href","file-cross.svg#icon")}}class i{constructor(t,i){this.loadFileCallback=t,this.loadedFiles=0;const a=(t,e,s,i,a=!0)=>[e,new t(document.getElementById(s),((t,s)=>{a&&(this.loadedFiles|=e),this.loadFileCallback(e,t,s)}),i)];this.fileInputs=new Map([a(s,1,"rom-input"),a(e,2,"import-save-input",void 0,!1),a(s,4,"bios7-input","bios7"),a(s,8,"bios9-input","bios9"),a(s,16,"fw-input","fw")]);for(const t of this.fileInputs.values())t.loadFromStorage();fetch("resources/game_db.json").then((t=>t.text())).then((t=>{this.gameDb=JSON.parse(t),i()}))}loaded(t){return(this.loadedFiles&t)===t}toggleEnabled(t,e){this.fileInputs.get(t).enabled=e}unloadRom(){this.loadedFiles&=-2,this.fileInputs.get(1).unload()}loadSaveFromStorage(t){this.fileInputs.get(2).loadFromStorage(`save-${t}`)}storeSaveToStorage(t,e,s){this.fileInputs.get(2).storeToStorage(t,e,`save-${s}`)}}class a{get halfWidth(){return this.halfWidth_}get halfHeight(){return this.halfHeight_}get x(){return this.x_}set x(t){this.x_=t,this.element.style.left=t-this.halfWidth_+"px"}get y(){return this.y_}set y(t){this.y_=t,this.element.style.top=t-this.halfHeight_+"px"}updateScale(){this.element.style.transform=`scale(${this.scale})`,this.updateInteractionScale()}get scale(){return this.scale_}set scale(t){this.scale_=t,this.halfWidth_=.5*this.element.clientWidth,this.halfHeight_=.5*this.element.clientHeight,this.updateScale()}updateInteractionScale(){const t=this.scale*this.interactionScale;this.interactionElement.style.transform=`translate(-50%, -50%) scale(${this.interactionScale})`,this.interactionElement.style.borderWidth=5/t+"px"}get interactionScale(){return this.interactionScale_}set interactionScale(t){this.interactionScale_=t,this.updateInteractionScale()}get editing(){return this.editing_}set editing(t){this.editing_=t,this.interactionElement.style.opacity=t?"1":"0"}get layoutData(){return{x:this.x_,y:this.y_,scale:this.scale_}}set layoutData(t){void 0!==t.x&&(this.x=t.x),void 0!==t.y&&(this.y=t.y),void 0!==t.scale&&(this.scale=t.scale),void 0!==t.interactionScale&&(this.interactionScale=t.interactionScale)}constructor(t,e={}){var s,i,a,o;this.element=t,this.editing_=!1,this.interactionElement=t.getElementsByClassName("interaction")[0],this.halfWidth_=.5*this.element.clientWidth,this.halfHeight_=.5*this.element.clientHeight,null!==(s=e.x)&&void 0!==s||(e.x=this.element.offsetLeft+this.halfWidth_),null!==(i=e.y)&&void 0!==i||(e.y=this.element.offsetTop+this.halfHeight_),null!==(a=e.scale)&&void 0!==a||(e.scale=1),null!==(o=e.interactionScale)&&void 0!==o||(e.interactionScale=1),this.layoutData=e}}class o extends a{constructor(t,e,s={}){super(t),this.stateBit=e,this.button=t.getElementsByTagName("button")[0],this.layoutData=s}}class n extends a{resetTouches(){this.up.classList.remove("pressed"),this.down.classList.remove("pressed"),this.left.classList.remove("pressed"),this.right.classList.remove("pressed")}processTouch(t,e){const s=Math.atan2(this.y-t.y,t.x-this.x),i=[16,80,64,96,32,160,128,144][7&Math.round(4*s/Math.PI)];return 64&i&&this.up.classList.add("pressed"),128&i&&this.down.classList.add("pressed"),32&i&&this.left.classList.add("pressed"),16&i&&this.right.classList.add("pressed"),e|i}constructor(t,e={}){var s;null!==(s=e.interactionScale)&&void 0!==s||(e.interactionScale=1.2),super(t,e),this.up=document.getElementById("dpad-up"),this.down=document.getElementById("dpad-down"),this.left=document.getElementById("dpad-left"),this.right=document.getElementById("dpad-right")}}class r{constructor(t,e={}){function s(t,s){return[t,new o(document.getElementById(`btn-${t}`),s,e[t])]}this.element=t,this.editing_=!1,this.buttons=new Map([s("a",1),s("b",2),s("x",65536),s("y",131072),s("l",512),s("r",256),s("start",8),s("select",4)]),this.dpad=new n(document.getElementById("dpad"),e.dpad),this.pause=new o(document.getElementById("btn-pause"),0,e.pause)}resetTouches(){for(const t of this.buttons.values())t.element.classList.remove("pressed");this.dpad.resetTouches()}containTouch(t,e){const s=document.elementsFromPoint(t,e);if(-1!==s.indexOf(this.pause.interactionElement))return!0;for(const t of this.buttons.values())if(-1!==s.indexOf(t.interactionElement))return!0;return-1!==s.indexOf(this.dpad.interactionElement)}processTouch(t,e){const s=document.elementsFromPoint(t.x,t.y);for(const t of this.buttons.values())-1!==s.indexOf(t.interactionElement)&&(e|=t.stateBit,t.element.classList.add("pressed"));return-1!==s.indexOf(this.dpad.interactionElement)&&(e=this.dpad.processTouch(t,e)),e}get layoutData(){const t={dpad:this.dpad.layoutData,pause:this.pause.layoutData};for(const[e,s]of this.buttons)t[e]=s.layoutData;return t}set layoutData(t){for(const[e,s]of this.buttons){const i=t[e];i&&(s.layoutData=i)}t.dpad&&(this.dpad.layoutData=t.dpad),t.pause&&(this.pause.layoutData=t.pause)}get defaultLayout(){const t=parseFloat(getComputedStyle(document.body).fontSize),e=document.body.clientWidth,s=document.body.clientHeight,i=e/s,a=(i<5/4?.4:i<4/3?.3:.1)*s,o=this.buttons.get("a"),n=this.buttons.get("b"),r=this.buttons.get("x"),h=this.buttons.get("y"),l=(o.halfWidth+n.halfWidth+r.halfWidth+h.halfWidth)/4,c=(o.halfHeight+n.halfHeight+r.halfHeight+h.halfHeight)/4,d=this.buttons.get("l"),u=this.buttons.get("r"),m=this.buttons.get("start"),g=this.buttons.get("select"),p=this.pause.halfWidth*this.pause.interactionScale,f=.5*e,b=2*this.dpad.halfWidth+2*t,v=e-(6*l+2*t),y=f+p+m.halfWidth+t,w=f-(p+g.halfWidth+t),E=y-m.halfWidth,x=b>=w-g.halfWidth||v<=E?s-(2*Math.max(m.halfHeight,g.halfHeight)+2*t):s-t;return{dpad:{x:this.dpad.halfWidth+t,y:x-this.dpad.halfHeight},a:{x:e-(l+t),y:x-3*c,interactionScale:1.75},b:{x:e-(3*l+t),y:x-c,interactionScale:1.75},x:{x:e-(3*l+t),y:x-5*c,interactionScale:1.75},y:{x:e-(5*l+t),y:x-3*c,interactionScale:1.75},l:{x:t+d.halfWidth,y:a+t+d.halfHeight},r:{x:e-(t+u.halfWidth),y:a+t+u.halfHeight},start:{x:y,y:s-(m.halfHeight+t)},select:{x:w,y:s-(g.halfHeight+t)},pause:{x:f,y:s-((this.element.classList.contains("touch")?Math.max(this.pause.halfHeight,(m.halfHeight+g.halfHeight)/2):this.pause.halfHeight)+t)}}}get editing(){return this.editing_}set editing(t){if(t!==this.editing_){this.editing_=t;for(const e of this.buttons.values())e.editing=t}}}const h={w:256,q:512,a:131072,s:65536,z:2,x:1,Enter:8,Shift:4,ArrowRight:16,ArrowLeft:32,ArrowDown:128,ArrowUp:64};var l;!function(t){t.fromParts=function(t,e,s,i){return{bottom:t,height:i,left:e,right:e+s,top:t-i,width:s,x:e,y:t-i}},t.contains=function(t,e,s){return e>=t.left&&e<t.right&&s>=t.top&&s<t.bottom}}(l||(l={}));class c{constructor(t,e){this.pauseCallback=e,this.touches=new Map,this.controls=document.getElementById("controls"),this.pauseButtonContainer=document.getElementById("btn-pause"),this.pauseButton=this.pauseButtonContainer.getElementsByTagName("button")[0],this.mouseDownCallback=this.mouseDown.bind(this),this.mouseMoveCallback=this.mouseMove.bind(this),this.mouseUpCallback=this.mouseUp.bind(this),this.touchStartCallback=this.touchStart.bind(this),this.touchMoveCallback=this.touchMove.bind(this),this.touchEndCallback=this.touchEnd.bind(this),this.prevInput=0,this.pressedKeys=0,document.body.addEventListener("keydown",(t=>{var e;this.pressedKeys|=null!==(e=h[t.key])&&void 0!==e?e:0})),document.body.addEventListener("keyup",(t=>{var e;this.pressedKeys&=~(null!==(e=h[t.key])&&void 0!==e?e:0)})),this.controls.addEventListener("mousedown",this.mouseDownCallback),this.controls.addEventListener("mousemove",this.mouseMoveCallback),window.addEventListener("mouseup",this.mouseUpCallback),this.controls.addEventListener("touchstart",this.touchStartCallback),this.controls.addEventListener("touchmove",this.touchMoveCallback),window.addEventListener("touchend",this.touchEndCallback),window.addEventListener("touchcancel",this.touchEndCallback),this.pauseButton.addEventListener("click",e),this.touch=t}get touch(){return this.touch_}set touch(t){t!==this.touch_&&(this.touch_=t,this.controls.classList.toggle("touch",t),t?(this.pauseButton.removeEventListener("click",this.pauseCallback),this.touchControls=new r(this.controls),this.touchControls.layoutData=this.touchControls.defaultLayout,this.touchControls.pause.interactionElement.addEventListener("click",this.pauseCallback)):(this.touchControls&&(this.touchControls.pause.interactionElement.removeEventListener("click",this.pauseCallback),delete this.touchControls),this.pauseButton.addEventListener("click",this.pauseCallback)))}setTouch(t,e,s){var i;this.touches.set(t,{area:(null===(i=this.touchControls)||void 0===i?void 0:i.containTouch(e,s))?0:1,startX:e,startY:s,x:e,y:s})}mouseDown(t){this.setTouch(-1,t.clientX,t.clientY)}touchStart(t){this.pauseButtonContainer.contains(t.target)||t.preventDefault();for(let e=0;e<t.changedTouches.length;++e){const s=t.changedTouches[e];this.setTouch(s.identifier,s.clientX,s.clientY)}}mouseMove(t){const e=this.touches.get(-1);e&&(e.x=t.clientX,e.y=t.clientY)}touchMove(t){this.pauseButtonContainer.contains(t.target)||t.preventDefault();for(let e=0;e<t.changedTouches.length;++e){const s=t.changedTouches[e],i=this.touches.get(s.identifier);i&&(i.x=s.clientX,i.y=s.clientY)}}mouseUp(t){this.touches.delete(-1)}touchEnd(t){for(let e=0;e<t.changedTouches.length;++e){const s=t.changedTouches[e];this.touches.delete(s.identifier)}}update(t){let e=this.pressedKeys;const s=.5*t.width,i=.5*t.height,a=t.x+s,o=t.y+i;let n=0,r=0,h=0;for(const e of this.touches.values()){if(2===e.area&&l.contains(t,e.x,e.y)&&(e.area=1),1!==e.area)continue;const c=e.x-a,d=e.y-o,u=Math.min(Math.abs(s/c),Math.abs(i/d),1);r+=s+c*u,h+=i+d*u,n++}const c=n?[Math.min(Math.floor(4096*r/(n*t.width)),4095),Math.min(Math.floor(3072*h/(n*t.height)),3071)]:null,d=(null==c?void 0:c[0])!==this.botScreenTouchX||(null==c?void 0:c[1])!==this.botScreenTouchY;if(this.botScreenRect=Object.assign({},t),this.botScreenTouchX=null==c?void 0:c[0],this.botScreenTouchY=null==c?void 0:c[1],this.touchControls){this.touchControls.resetTouches();for(const t of this.touches.values())0===t.area&&(e=this.touchControls.processTouch(t,e))}const u=this.prevInput;return this.prevInput=e,e!==u||d?{pressed:e&~u,released:u&~e,touchPos:d?c:void 0}:null}}new class{constructor(t){this.canvasContainer=document.getElementById("canvas-container"),this.canvas=document.getElementById("canvas"),this.input=new c(t,this.pause.bind(this)),this.audio=new(window.AudioContext||window.webkitAudioContext),this.audioTime=0;const e=()=>{this.audio.resume(),document.removeEventListener("touchstart",e),document.removeEventListener("touchend",e)};document.addEventListener("touchstart",e),document.addEventListener("touchend",e),document.addEventListener("visibilitychange",(()=>{this.worker&&("visible"===document.visibilityState?this.play():this.pause())})),this.exportSaveButton=document.getElementById("export-save"),this.playButton=document.getElementById("play"),this.stopButton=document.getElementById("stop"),this.resetButton=document.getElementById("reset"),this.limitFramerateCheckbox=document.getElementById("toggle-framerate-limit"),this.touchControlsCheckbox=document.getElementById("toggle-touch-controls"),this.files=new i(((t,e,s)=>{switch(t){case 1:this.queueRomLoad(e,new Uint8Array(s));break;case 2:this.loadSave(e,s);break;case 4:this.bios7=new Uint8Array(s);break;case 8:this.bios9=new Uint8Array(s);break;case 16:this.firmware=new Uint8Array(s)}}),(()=>{this.toggleRomEnabledIfSystemFilesLoaded()})),this.exportSaveButton.addEventListener("click",this.requestSaveExport.bind(this)),this.playButton.addEventListener("click",this.play.bind(this)),this.stopButton.addEventListener("click",this.requestStop.bind(this)),this.resetButton.addEventListener("click",this.reset.bind(this)),this.limitFramerateCheckbox.addEventListener("change",(t=>{this.setFramerateLimit(this.limitFramerateCheckbox.checked)})),this.touchControlsCheckbox.checked=t,this.touchControlsCheckbox.addEventListener("change",(t=>{this.input.touch=this.touchControlsCheckbox.checked}));const s=this.canvas.getContext("webgl",{alpha:!1,depth:!1,stencil:!1,antialias:!1,powerPreference:"low-power"});if(!s)throw new Error("Couldn't create WebGL context");this.gl=s;const a=s.createTexture();s.bindTexture(s.TEXTURE_2D,a),s.texParameteri(s.TEXTURE_2D,s.TEXTURE_MIN_FILTER,s.LINEAR),s.texParameteri(s.TEXTURE_2D,s.TEXTURE_MAG_FILTER,s.NEAREST),s.texParameteri(s.TEXTURE_2D,s.TEXTURE_WRAP_S,s.CLAMP_TO_EDGE),s.texParameteri(s.TEXTURE_2D,s.TEXTURE_WRAP_T,s.CLAMP_TO_EDGE),s.texImage2D(s.TEXTURE_2D,0,s.RGBA,256,384,0,s.RGBA,s.UNSIGNED_BYTE,new Uint8Array(393216));const o=s.createShader(s.VERTEX_SHADER);if(s.shaderSource(o,"attribute vec2 coords;\n\nvarying vec2 tex_coord;\n\nvoid main() {\n    tex_coord = (vec2(coords.x, -coords.y) + 1.0) * 0.5;\n    gl_Position = vec4(coords.x, coords.y, 0.0, 1.0);\n}\n"),s.compileShader(o),!s.getShaderParameter(o,s.COMPILE_STATUS))throw new Error(`WebGL vertex shader compilation failed: ${s.getShaderInfoLog(o)}`);const n=s.createShader(s.FRAGMENT_SHADER);if(s.shaderSource(n,"precision mediump float;\n\nuniform sampler2D framebuffer;\n\nvarying vec2 tex_coord;\n\nvoid main() {\n    gl_FragColor = vec4(texture2D(framebuffer, tex_coord).rgb, 1.0);\n}\n"),s.compileShader(n),!s.getShaderParameter(n,s.COMPILE_STATUS))throw new Error(`WebGL fragment shader compilation failed: ${s.getShaderInfoLog(n)}`);const r=s.createProgram();if(s.attachShader(r,o),s.attachShader(r,n),s.linkProgram(r),!s.getProgramParameter(r,s.LINK_STATUS))throw new Error(`WebGL program linking failed: ${s.getProgramInfoLog(r)}`);this.fbProgram=r,this.fbCoordsAttrib=s.getAttribLocation(r,"coords");const h=s.createBuffer();s.bindBuffer(s.ARRAY_BUFFER,h),s.bufferData(s.ARRAY_BUFFER,new Float32Array([-1,-1,1,-1,-1,1,1,1]),s.STATIC_DRAW),this.playing=!1,this.frame(),window.addEventListener("beforeunload",(()=>{this.worker&&this.sendMessage({type:2})}))}toggleRomEnabledIfSystemFilesLoaded(){var t;(null===(t=this.files)||void 0===t?void 0:t.gameDb)&&this.files.toggleEnabled(1,!0)}sendMessage(t,e){this.worker.postMessage(t,e)}handleStartingWorkerMessage(e){if(0!==e.data.type)return;this.files.toggleEnabled(2,!0),this.exportSaveButton.disabled=!1,this.playButton.disabled=!1,this.stopButton.disabled=!1,this.resetButton.disabled=!1,this.limitFramerateCheckbox.disabled=!1,this.limitFramerateCheckbox.checked=!0;const s=this.nextRomFilename.lastIndexOf(".");this.gameTitle=-1===s?this.nextRomFilename:this.nextRomFilename.slice(0,s),this.saveFilename=`${this.gameTitle}.sav`;const i=new Uint32Array(this.nextRomBuffer.buffer,0,this.nextRomBuffer.length>>2)[3];let a;const o=function(t,e){let s=0,i=t.length-1;for(;s!==i;){const a=s+i>>1,o=t[a];if(o.code>e)i=a;else{if(!(o.code<e))return o;s=a+1}}}(this.files.gameDb,i);o&&(this.nextRomBuffer.length!==o["rom-size"]&&console.warn(`Unexpected ROM size: expected ${o["rom-size"]} B, got ${this.nextRomBuffer.length} B`),a=t[o["save-type"]]),this.sendMessage({type:0,rom:this.nextRomBuffer,bios7:this.bios7,bios9:this.bios9,firmware:this.firmware,saveType:a,hasIR:73==(255&i)},[this.nextRomBuffer.buffer]),this.files.loadSaveFromStorage(this.gameTitle),this.nextRomFilename=void 0,this.nextRomBuffer=void 0,this.worker.onmessage=this.handleWorkerMessage.bind(this)}handleWorkerMessage(t){const e=t.data;switch(e.type){case 1:this.rendererWorker=new Worker("renderer_3d.bundle.js"),this.rendererWorker.postMessage({module:e.module,memory:e.memory});break;case 2:if(this.files.storeSaveToStorage(this.saveFilename,e.buffer,this.gameTitle),e.triggerDownload){const t=new Blob([e.buffer],{type:"application/octet-stream;charset=utf-8"}),s=document.createElement("a"),i=URL.createObjectURL(t);s.href=i,s.download=this.saveFilename,document.body.appendChild(s),s.onclick=()=>s.remove(),s.click(),URL.revokeObjectURL(i)}break;case 3:this.gl.texSubImage2D(this.gl.TEXTURE_2D,0,0,0,256,384,this.gl.RGBA,this.gl.UNSIGNED_BYTE,new Uint8Array(e.buffer.buffer));break;case 5:{const t=1<<25,s=t/1024*60/(t/560190),i=this.audio.currentTime+(this.audio.baseLatency||1/60);if(this.audioTime>i+e.l.length/s)break;this.audioTime<i&&(this.audioTime=i);const a=this.audio.createBuffer(2,e.l.length,s);a.copyToChannel?(a.copyToChannel(e.l,0),a.copyToChannel(e.r,1)):(a.getChannelData(0).set(e.l),a.getChannelData(1).set(e.r));const o=this.audio.createBufferSource();o.buffer=a,o.connect(this.audio.destination),o.start?o.start(this.audioTime):o.noteOn&&o.noteOn(this.audioTime),this.audioTime+=e.l.length/s;break}}}handleClosingWorkerMessage(t){const e=t.data;4===e.type&&(this.worker=void 0,this.files.storeSaveToStorage(this.saveFilename,e.buffer,this.gameTitle),this.saveFilename=void 0,this.gameTitle=void 0,this.tryStartQueuedWorker())}requestStop(){this.worker&&(this.files.toggleEnabled(2,!1),this.exportSaveButton.disabled=!0,this.playButton.disabled=!0,this.stopButton.disabled=!0,this.resetButton.disabled=!0,this.limitFramerateCheckbox.disabled=!0,this.sendMessage({type:2}),this.worker.onmessage=this.handleClosingWorkerMessage.bind(this),this.files.unloadRom(),this.gl.texSubImage2D(this.gl.TEXTURE_2D,0,0,0,256,384,this.gl.RGBA,this.gl.UNSIGNED_BYTE,new Uint8Array(393216)))}tryStartQueuedWorker(){this.nextRomFilename&&this.nextRomBuffer&&(this.worker=new Worker("emu.bundle.js"),this.worker.onmessage=this.handleStartingWorkerMessage.bind(this))}queueRomLoad(t,e){this.nextRomFilename=t,this.nextRomBuffer=e,this.worker?this.requestStop():this.tryStartQueuedWorker()}reset(){this.sendMessage({type:1})}requestSaveExport(){this.sendMessage({type:4})}setFramerateLimit(t){this.sendMessage({type:7,value:t})}loadSave(t,e){this.files.storeSaveToStorage(t,e,this.gameTitle),this.saveFilename=t,this.sendMessage({type:3,buffer:e},[e])}frame(){if(this.playing){const t=this.canvasContainer.clientWidth,e=this.canvasContainer.clientHeight,s=256/384;let i=Math.floor(Math.min(e*s,t)),a=Math.floor(i/s);this.canvas.style.width=`${i}px`,this.canvas.style.height=`${a}px`,i*=window.devicePixelRatio,a*=window.devicePixelRatio,this.canvas.width=i,this.canvas.height=a,this.gl.viewport(0,0,i,a),this.gl.clearColor(0,0,0,1),this.gl.clear(this.gl.COLOR_BUFFER_BIT),this.gl.useProgram(this.fbProgram),this.gl.vertexAttribPointer(this.fbCoordsAttrib,2,this.gl.FLOAT,!1,8,0),this.gl.enableVertexAttribArray(this.fbCoordsAttrib),this.gl.drawArrays(this.gl.TRIANGLE_STRIP,0,4)}const t=this.canvas.getBoundingClientRect(),e=this.input.update(l.fromParts(t.bottom,t.left,t.width,.5*t.height));this.worker&&e&&this.sendMessage(Object.assign({type:5},e)),requestAnimationFrame(this.frame.bind(this))}play(){document.body.classList.remove("paused"),this.playing=!0,this.sendMessage({type:6,value:!0})}pause(){document.body.classList.add("paused"),this.playing=!1,this.sendMessage({type:6,value:!1})}}(function(){const t=navigator.userAgent||navigator.vendor||window.opera;return/(android|bb\d+|meego).+mobile|avantgo|bada\/|blackberry|blazer|compal|elaine|fennec|hiptop|iemobile|ip(hone|od)|iris|kindle|lge |maemo|midp|mmp|mobile.+firefox|netfront|opera m(ob|in)i|palm( os)?|phone|p(ixi|re)\/|plucker|pocket|psp|series(4|6)0|symbian|treo|up\.(browser|link)|vodafone|wap|windows ce|xda|xiino/i.test(t)||/1207|6310|6590|3gso|4thp|50[1-6]i|770s|802s|a wa|abac|ac(er|oo|s\-)|ai(ko|rn)|al(av|ca|co)|amoi|an(ex|ny|yw)|aptu|ar(ch|go)|as(te|us)|attw|au(di|\-m|r |s )|avan|be(ck|ll|nq)|bi(lb|rd)|bl(ac|az)|br(e|v)w|bumb|bw\-(n|u)|c55\/|capi|ccwa|cdm\-|cell|chtm|cldc|cmd\-|co(mp|nd)|craw|da(it|ll|ng)|dbte|dc\-s|devi|dica|dmob|do(c|p)o|ds(12|\-d)|el(49|ai)|em(l2|ul)|er(ic|k0)|esl8|ez([4-7]0|os|wa|ze)|fetc|fly(\-|_)|g1 u|g560|gene|gf\-5|g\-mo|go(\.w|od)|gr(ad|un)|haie|hcit|hd\-(m|p|t)|hei\-|hi(pt|ta)|hp( i|ip)|hs\-c|ht(c(\-| |_|a|g|p|s|t)|tp)|hu(aw|tc)|i\-(20|go|ma)|i230|iac( |\-|\/)|ibro|idea|ig01|ikom|im1k|inno|ipaq|iris|ja(t|v)a|jbro|jemu|jigs|kddi|keji|kgt( |\/)|klon|kpt |kwc\-|kyo(c|k)|le(no|xi)|lg( g|\/(k|l|u)|50|54|\-[a-w])|libw|lynx|m1\-w|m3ga|m50\/|ma(te|ui|xo)|mc(01|21|ca)|m\-cr|me(rc|ri)|mi(o8|oa|ts)|mmef|mo(01|02|bi|de|do|t(\-| |o|v)|zz)|mt(50|p1|v )|mwbp|mywa|n10[0-2]|n20[2-3]|n30(0|2)|n50(0|2|5)|n7(0(0|1)|10)|ne((c|m)\-|on|tf|wf|wg|wt)|nok(6|i)|nzph|o2im|op(ti|wv)|oran|owg1|p800|pan(a|d|t)|pdxg|pg(13|\-([1-8]|c))|phil|pire|pl(ay|uc)|pn\-2|po(ck|rt|se)|prox|psio|pt\-g|qa\-a|qc(07|12|21|32|60|\-[2-7]|i\-)|qtek|r380|r600|raks|rim9|ro(ve|zo)|s55\/|sa(ge|ma|mm|ms|ny|va)|sc(01|h\-|oo|p\-)|sdk\/|se(c(\-|0|1)|47|mc|nd|ri)|sgh\-|shar|sie(\-|m)|sk\-0|sl(45|id)|sm(al|ar|b3|it|t5)|so(ft|ny)|sp(01|h\-|v\-|v )|sy(01|mb)|t2(18|50)|t6(00|10|18)|ta(gt|lk)|tcl\-|tdg\-|tel(i|m)|tim\-|t\-mo|to(pl|sh)|ts(70|m\-|m3|m5)|tx\-9|up(\.b|g1|si)|utst|v400|v750|veri|vi(rg|te)|vk(40|5[0-3]|\-v)|vm40|voda|vulc|vx(52|53|60|61|70|80|81|83|85|98)|w3c(\-| )|webc|whit|wi(g |nc|nw)|wmlb|wonu|x700|yas\-|your|zeto|zte\-/i.test(t.substr(0,4))}())})()})();