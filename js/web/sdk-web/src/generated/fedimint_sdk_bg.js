let wasm
export function __wbg_set_wasm(val) {
  wasm = val
}

function addToExternrefTable0(obj) {
  const idx = wasm.__externref_table_alloc()
  wasm.__wbindgen_externrefs.set(idx, obj)
  return idx
}

const CLOSURE_DTORS =
  typeof FinalizationRegistry === 'undefined'
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry((state) => state.dtor(state.a, state.b))

function debugString(val) {
  // primitive types
  const type = typeof val
  if (type == 'number' || type == 'boolean' || val == null) {
    return `${val}`
  }
  if (type == 'string') {
    return `"${val}"`
  }
  if (type == 'symbol') {
    const description = val.description
    if (description == null) {
      return 'Symbol'
    } else {
      return `Symbol(${description})`
    }
  }
  if (type == 'function') {
    const name = val.name
    if (typeof name == 'string' && name.length > 0) {
      return `Function(${name})`
    } else {
      return 'Function'
    }
  }
  // objects
  if (Array.isArray(val)) {
    const length = val.length
    let debug = '['
    if (length > 0) {
      debug += debugString(val[0])
    }
    for (let i = 1; i < length; i++) {
      debug += ', ' + debugString(val[i])
    }
    debug += ']'
    return debug
  }
  // Test for built-in
  const builtInMatches = /\[object ([^\]]+)\]/.exec(toString.call(val))
  let className
  if (builtInMatches && builtInMatches.length > 1) {
    className = builtInMatches[1]
  } else {
    // Failed to match the standard '[object ClassName]'
    return toString.call(val)
  }
  if (className == 'Object') {
    // we're a user defined class or Object
    // JSON.stringify avoids problems with cycles, and is generally much
    // easier than looping through ownProperties of `val`.
    try {
      return 'Object(' + JSON.stringify(val) + ')'
    } catch (_) {
      return 'Object'
    }
  }
  // errors
  if (val instanceof Error) {
    return `${val.name}: ${val.message}\n${val.stack}`
  }
  // TODO we could test for more things here, like `Set`s and `Map`s.
  return className
}

function getArrayU8FromWasm0(ptr, len) {
  ptr = ptr >>> 0
  return getUint8ArrayMemory0().subarray(ptr / 1, ptr / 1 + len)
}

let cachedDataViewMemory0 = null
function getDataViewMemory0() {
  if (
    cachedDataViewMemory0 === null ||
    cachedDataViewMemory0.buffer.detached === true ||
    (cachedDataViewMemory0.buffer.detached === undefined &&
      cachedDataViewMemory0.buffer !== wasm.memory.buffer)
  ) {
    cachedDataViewMemory0 = new DataView(wasm.memory.buffer)
  }
  return cachedDataViewMemory0
}

function getStringFromWasm0(ptr, len) {
  ptr = ptr >>> 0
  return decodeText(ptr, len)
}

let cachedUint8ArrayMemory0 = null
function getUint8ArrayMemory0() {
  if (
    cachedUint8ArrayMemory0 === null ||
    cachedUint8ArrayMemory0.byteLength === 0
  ) {
    cachedUint8ArrayMemory0 = new Uint8Array(wasm.memory.buffer)
  }
  return cachedUint8ArrayMemory0
}

function handleError(f, args) {
  try {
    return f.apply(this, args)
  } catch (e) {
    const idx = addToExternrefTable0(e)
    wasm.__wbindgen_exn_store(idx)
  }
}

function isLikeNone(x) {
  return x === undefined || x === null
}

function makeClosure(arg0, arg1, dtor, f) {
  const state = { a: arg0, b: arg1, cnt: 1, dtor }
  const real = (...args) => {
    // First up with a closure we increment the internal reference
    // count. This ensures that the Rust closure environment won't
    // be deallocated while we're invoking it.
    state.cnt++
    try {
      return f(state.a, state.b, ...args)
    } finally {
      real._wbg_cb_unref()
    }
  }
  real._wbg_cb_unref = () => {
    if (--state.cnt === 0) {
      state.dtor(state.a, state.b)
      state.a = 0
      CLOSURE_DTORS.unregister(state)
    }
  }
  CLOSURE_DTORS.register(real, state, state)
  return real
}

function makeMutClosure(arg0, arg1, dtor, f) {
  const state = { a: arg0, b: arg1, cnt: 1, dtor }
  const real = (...args) => {
    // First up with a closure we increment the internal reference
    // count. This ensures that the Rust closure environment won't
    // be deallocated while we're invoking it.
    state.cnt++
    const a = state.a
    state.a = 0
    try {
      return f(a, state.b, ...args)
    } finally {
      state.a = a
      real._wbg_cb_unref()
    }
  }
  real._wbg_cb_unref = () => {
    if (--state.cnt === 0) {
      state.dtor(state.a, state.b)
      state.a = 0
      CLOSURE_DTORS.unregister(state)
    }
  }
  CLOSURE_DTORS.register(real, state, state)
  return real
}

function passStringToWasm0(arg, malloc, realloc) {
  if (realloc === undefined) {
    const buf = cachedTextEncoder.encode(arg)
    const ptr = malloc(buf.length, 1) >>> 0
    getUint8ArrayMemory0()
      .subarray(ptr, ptr + buf.length)
      .set(buf)
    WASM_VECTOR_LEN = buf.length
    return ptr
  }

  let len = arg.length
  let ptr = malloc(len, 1) >>> 0

  const mem = getUint8ArrayMemory0()

  let offset = 0

  for (; offset < len; offset++) {
    const code = arg.charCodeAt(offset)
    if (code > 0x7f) break
    mem[ptr + offset] = code
  }
  if (offset !== len) {
    if (offset !== 0) {
      arg = arg.slice(offset)
    }
    ptr = realloc(ptr, len, (len = offset + arg.length * 3), 1) >>> 0
    const view = getUint8ArrayMemory0().subarray(ptr + offset, ptr + len)
    const ret = cachedTextEncoder.encodeInto(arg, view)

    offset += ret.written
    ptr = realloc(ptr, len, offset, 1) >>> 0
  }

  WASM_VECTOR_LEN = offset
  return ptr
}

let cachedTextDecoder = new TextDecoder('utf-8', {
  ignoreBOM: true,
  fatal: true,
})
cachedTextDecoder.decode()
const MAX_SAFARI_DECODE_BYTES = 2146435072
let numBytesDecoded = 0
function decodeText(ptr, len) {
  numBytesDecoded += len
  if (numBytesDecoded >= MAX_SAFARI_DECODE_BYTES) {
    cachedTextDecoder = new TextDecoder('utf-8', {
      ignoreBOM: true,
      fatal: true,
    })
    cachedTextDecoder.decode()
    numBytesDecoded = len
  }
  return cachedTextDecoder.decode(
    getUint8ArrayMemory0().subarray(ptr, ptr + len),
  )
}

const cachedTextEncoder = new TextEncoder()

if (!('encodeInto' in cachedTextEncoder)) {
  cachedTextEncoder.encodeInto = function (arg, view) {
    const buf = cachedTextEncoder.encode(arg)
    view.set(buf)
    return {
      read: arg.length,
      written: buf.length,
    }
  }
}

let WASM_VECTOR_LEN = 0

function wasm_bindgen__convert__closures_____invoke__hb5c66d63acb43222(
  arg0,
  arg1,
) {
  wasm.wasm_bindgen__convert__closures_____invoke__hb5c66d63acb43222(arg0, arg1)
}

function wasm_bindgen__convert__closures_____invoke__h732e77c759a01739(
  arg0,
  arg1,
  arg2,
) {
  wasm.wasm_bindgen__convert__closures_____invoke__h732e77c759a01739(
    arg0,
    arg1,
    arg2,
  )
}

function wasm_bindgen__convert__closures_____invoke__hebe6e299b2388f3c(
  arg0,
  arg1,
) {
  wasm.wasm_bindgen__convert__closures_____invoke__hebe6e299b2388f3c(arg0, arg1)
}

function wasm_bindgen__convert__closures_____invoke__h637d0d38d8ce5792(
  arg0,
  arg1,
  arg2,
) {
  wasm.wasm_bindgen__convert__closures_____invoke__h637d0d38d8ce5792(
    arg0,
    arg1,
    arg2,
  )
}

function wasm_bindgen__convert__closures_____invoke__hed3a942a68768433(
  arg0,
  arg1,
) {
  wasm.wasm_bindgen__convert__closures_____invoke__hed3a942a68768433(arg0, arg1)
}

function wasm_bindgen__convert__closures_____invoke__hfec9849f3ac1bcdf(
  arg0,
  arg1,
) {
  wasm.wasm_bindgen__convert__closures_____invoke__hfec9849f3ac1bcdf(arg0, arg1)
}

function wasm_bindgen__convert__closures_____invoke__h9255eddd6f35e8b7(
  arg0,
  arg1,
) {
  wasm.wasm_bindgen__convert__closures_____invoke__h9255eddd6f35e8b7(arg0, arg1)
}

function wasm_bindgen__convert__closures_____invoke__h3e3bf5ab6c41937b(
  arg0,
  arg1,
  arg2,
) {
  wasm.wasm_bindgen__convert__closures_____invoke__h3e3bf5ab6c41937b(
    arg0,
    arg1,
    arg2,
  )
}

function wasm_bindgen__convert__closures_____invoke__h00dbb47cb783c1cb(
  arg0,
  arg1,
  arg2,
) {
  wasm.wasm_bindgen__convert__closures_____invoke__h00dbb47cb783c1cb(
    arg0,
    arg1,
    arg2,
  )
}

function wasm_bindgen__convert__closures_____invoke__h07593877f7995c23(
  arg0,
  arg1,
) {
  wasm.wasm_bindgen__convert__closures_____invoke__h07593877f7995c23(arg0, arg1)
}

function wasm_bindgen__convert__closures_____invoke__h81332cdc358993a9(
  arg0,
  arg1,
) {
  wasm.wasm_bindgen__convert__closures_____invoke__h81332cdc358993a9(arg0, arg1)
}

function wasm_bindgen__convert__closures_____invoke__hcbcb149b9e49fb4e(
  arg0,
  arg1,
  arg2,
) {
  wasm.wasm_bindgen__convert__closures_____invoke__hcbcb149b9e49fb4e(
    arg0,
    arg1,
    arg2,
  )
}

function wasm_bindgen__convert__closures_____invoke__hac25bb55b8468f62(
  arg0,
  arg1,
) {
  wasm.wasm_bindgen__convert__closures_____invoke__hac25bb55b8468f62(arg0, arg1)
}

function wasm_bindgen__convert__closures_____invoke__h9537e2360576635a(
  arg0,
  arg1,
  arg2,
  arg3,
) {
  wasm.wasm_bindgen__convert__closures_____invoke__h9537e2360576635a(
    arg0,
    arg1,
    arg2,
    arg3,
  )
}

const __wbindgen_enum_BinaryType = ['blob', 'arraybuffer']

const __wbindgen_enum_ReadableStreamType = ['bytes']

const __wbindgen_enum_RequestCache = [
  'default',
  'no-store',
  'reload',
  'no-cache',
  'force-cache',
  'only-if-cached',
]

const __wbindgen_enum_RequestCredentials = ['omit', 'same-origin', 'include']

const __wbindgen_enum_RequestMode = [
  'same-origin',
  'no-cors',
  'cors',
  'navigate',
]

const IntoUnderlyingByteSourceFinalization =
  typeof FinalizationRegistry === 'undefined'
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry((ptr) =>
        wasm.__wbg_intounderlyingbytesource_free(ptr >>> 0, 1),
      )

const IntoUnderlyingSinkFinalization =
  typeof FinalizationRegistry === 'undefined'
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry((ptr) =>
        wasm.__wbg_intounderlyingsink_free(ptr >>> 0, 1),
      )

const IntoUnderlyingSourceFinalization =
  typeof FinalizationRegistry === 'undefined'
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry((ptr) =>
        wasm.__wbg_intounderlyingsource_free(ptr >>> 0, 1),
      )

export class IntoUnderlyingByteSource {
  __destroy_into_raw() {
    const ptr = this.__wbg_ptr
    this.__wbg_ptr = 0
    IntoUnderlyingByteSourceFinalization.unregister(this)
    return ptr
  }
  free() {
    const ptr = this.__destroy_into_raw()
    wasm.__wbg_intounderlyingbytesource_free(ptr, 0)
  }
  /**
   * @returns {number}
   */
  get autoAllocateChunkSize() {
    const ret = wasm.intounderlyingbytesource_autoAllocateChunkSize(
      this.__wbg_ptr,
    )
    return ret >>> 0
  }
  /**
   * @param {ReadableByteStreamController} controller
   * @returns {Promise<any>}
   */
  pull(controller) {
    const ret = wasm.intounderlyingbytesource_pull(this.__wbg_ptr, controller)
    return ret
  }
  /**
   * @param {ReadableByteStreamController} controller
   */
  start(controller) {
    wasm.intounderlyingbytesource_start(this.__wbg_ptr, controller)
  }
  /**
   * @returns {ReadableStreamType}
   */
  get type() {
    const ret = wasm.intounderlyingbytesource_type(this.__wbg_ptr)
    return __wbindgen_enum_ReadableStreamType[ret]
  }
  cancel() {
    const ptr = this.__destroy_into_raw()
    wasm.intounderlyingbytesource_cancel(ptr)
  }
}
if (Symbol.dispose)
  IntoUnderlyingByteSource.prototype[Symbol.dispose] =
    IntoUnderlyingByteSource.prototype.free

export class IntoUnderlyingSink {
  __destroy_into_raw() {
    const ptr = this.__wbg_ptr
    this.__wbg_ptr = 0
    IntoUnderlyingSinkFinalization.unregister(this)
    return ptr
  }
  free() {
    const ptr = this.__destroy_into_raw()
    wasm.__wbg_intounderlyingsink_free(ptr, 0)
  }
  /**
   * @param {any} reason
   * @returns {Promise<any>}
   */
  abort(reason) {
    const ptr = this.__destroy_into_raw()
    const ret = wasm.intounderlyingsink_abort(ptr, reason)
    return ret
  }
  /**
   * @returns {Promise<any>}
   */
  close() {
    const ptr = this.__destroy_into_raw()
    const ret = wasm.intounderlyingsink_close(ptr)
    return ret
  }
  /**
   * @param {any} chunk
   * @returns {Promise<any>}
   */
  write(chunk) {
    const ret = wasm.intounderlyingsink_write(this.__wbg_ptr, chunk)
    return ret
  }
}
if (Symbol.dispose)
  IntoUnderlyingSink.prototype[Symbol.dispose] =
    IntoUnderlyingSink.prototype.free

export class IntoUnderlyingSource {
  __destroy_into_raw() {
    const ptr = this.__wbg_ptr
    this.__wbg_ptr = 0
    IntoUnderlyingSourceFinalization.unregister(this)
    return ptr
  }
  free() {
    const ptr = this.__destroy_into_raw()
    wasm.__wbg_intounderlyingsource_free(ptr, 0)
  }
  /**
   * @param {ReadableStreamDefaultController} controller
   * @returns {Promise<any>}
   */
  pull(controller) {
    const ret = wasm.intounderlyingsource_pull(this.__wbg_ptr, controller)
    return ret
  }
  cancel() {
    const ptr = this.__destroy_into_raw()
    wasm.intounderlyingsource_cancel(ptr)
  }
}
if (Symbol.dispose)
  IntoUnderlyingSource.prototype[Symbol.dispose] =
    IntoUnderlyingSource.prototype.free

export function __wbg___wbindgen_boolean_get_dea25b33882b895b(arg0) {
  const v = arg0
  const ret = typeof v === 'boolean' ? v : undefined
  return isLikeNone(ret) ? 0xffffff : ret ? 1 : 0
}

export function __wbg___wbindgen_debug_string_adfb662ae34724b6(arg0, arg1) {
  const ret = debugString(arg1)
  const ptr1 = passStringToWasm0(
    ret,
    wasm.__wbindgen_malloc,
    wasm.__wbindgen_realloc,
  )
  const len1 = WASM_VECTOR_LEN
  getDataViewMemory0().setInt32(arg0 + 4 * 1, len1, true)
  getDataViewMemory0().setInt32(arg0 + 4 * 0, ptr1, true)
}

export function __wbg___wbindgen_is_function_8d400b8b1af978cd(arg0) {
  const ret = typeof arg0 === 'function'
  return ret
}

export function __wbg___wbindgen_is_object_ce774f3490692386(arg0) {
  const val = arg0
  const ret = typeof val === 'object' && val !== null
  return ret
}

export function __wbg___wbindgen_is_string_704ef9c8fc131030(arg0) {
  const ret = typeof arg0 === 'string'
  return ret
}

export function __wbg___wbindgen_is_undefined_f6b95eab589e0269(arg0) {
  const ret = arg0 === undefined
  return ret
}

export function __wbg___wbindgen_string_get_a2a31e16edf96e42(arg0, arg1) {
  const obj = arg1
  const ret = typeof obj === 'string' ? obj : undefined
  var ptr1 = isLikeNone(ret)
    ? 0
    : passStringToWasm0(ret, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc)
  var len1 = WASM_VECTOR_LEN
  getDataViewMemory0().setInt32(arg0 + 4 * 1, len1, true)
  getDataViewMemory0().setInt32(arg0 + 4 * 0, ptr1, true)
}

export function __wbg___wbindgen_throw_dd24417ed36fc46e(arg0, arg1) {
  throw new Error(getStringFromWasm0(arg0, arg1))
}

export function __wbg__wbg_cb_unref_87dfb5aaa0cbcea7(arg0) {
  arg0._wbg_cb_unref()
}

export function __wbg_abort_07646c894ebbf2bd(arg0) {
  arg0.abort()
}

export function __wbg_abort_399ecbcfd6ef3c8e(arg0, arg1) {
  arg0.abort(arg1)
}

export function __wbg_addEventListener_6a82629b3d430a48() {
  return handleError(function (arg0, arg1, arg2, arg3) {
    arg0.addEventListener(getStringFromWasm0(arg1, arg2), arg3)
  }, arguments)
}

export function __wbg_addEventListener_82cddc614107eb45() {
  return handleError(function (arg0, arg1, arg2, arg3, arg4) {
    arg0.addEventListener(getStringFromWasm0(arg1, arg2), arg3, arg4)
  }, arguments)
}

export function __wbg_addEventListener_e792423147a80626() {
  return handleError(function (arg0, arg1, arg2, arg3) {
    arg0.addEventListener(getStringFromWasm0(arg1, arg2), arg3)
  }, arguments)
}

export function __wbg_append_c5cbdf46455cc776() {
  return handleError(function (arg0, arg1, arg2, arg3, arg4) {
    arg0.append(getStringFromWasm0(arg1, arg2), getStringFromWasm0(arg3, arg4))
  }, arguments)
}

export function __wbg_arrayBuffer_c04af4fce566092d() {
  return handleError(function (arg0) {
    const ret = arg0.arrayBuffer()
    return ret
  }, arguments)
}

export function __wbg_body_947b901c33f7fe32(arg0) {
  const ret = arg0.body
  return isLikeNone(ret) ? 0 : addToExternrefTable0(ret)
}

export function __wbg_buffer_6cb2fecb1f253d71(arg0) {
  const ret = arg0.buffer
  return ret
}

export function __wbg_byobRequest_f8e3517f5f8ad284(arg0) {
  const ret = arg0.byobRequest
  return isLikeNone(ret) ? 0 : addToExternrefTable0(ret)
}

export function __wbg_byteLength_faa9938885bdeee6(arg0) {
  const ret = arg0.byteLength
  return ret
}

export function __wbg_byteOffset_3868b6a19ba01dea(arg0) {
  const ret = arg0.byteOffset
  return ret
}

export function __wbg_call_3020136f7a2d6e44() {
  return handleError(function (arg0, arg1, arg2) {
    const ret = arg0.call(arg1, arg2)
    return ret
  }, arguments)
}

export function __wbg_call_abb4ff46ce38be40() {
  return handleError(function (arg0, arg1) {
    const ret = arg0.call(arg1)
    return ret
  }, arguments)
}

export function __wbg_cancel_a65cf45dca50ba4c(arg0) {
  const ret = arg0.cancel()
  return ret
}

export function __wbg_catch_b9db41d97d42bd02(arg0, arg1) {
  const ret = arg0.catch(arg1)
  return ret
}

export function __wbg_clearTimeout_15dfc3d1dcb635c6() {
  return handleError(function (arg0, arg1) {
    arg0.clearTimeout(arg1)
  }, arguments)
}

export function __wbg_clearTimeout_3b5c565a5ec539dd(arg0) {
  const ret = clearTimeout(arg0)
  return ret
}

export function __wbg_clearTimeout_42d9ccd50822fd3a(arg0) {
  const ret = clearTimeout(arg0)
  return ret
}

export function __wbg_clearTimeout_5e42188b495715bb() {
  return handleError(function (arg0, arg1) {
    arg0.clearTimeout(arg1)
  }, arguments)
}

export function __wbg_clearTimeout_96804de0ab838f26(arg0) {
  const ret = clearTimeout(arg0)
  return ret
}

export function __wbg_close_0af5661bf3d335f2() {
  return handleError(function (arg0) {
    arg0.close()
  }, arguments)
}

export function __wbg_close_1db3952de1b5b1cf() {
  return handleError(function (arg0) {
    arg0.close()
  }, arguments)
}

export function __wbg_close_3ec111e7b23d94d8() {
  return handleError(function (arg0) {
    arg0.close()
  }, arguments)
}

export function __wbg_code_85a811fe6ca962be(arg0) {
  const ret = arg0.code
  return ret
}

export function __wbg_code_c2a85f2863ec11b3(arg0) {
  const ret = arg0.code
  return ret
}

export function __wbg_createSyncAccessHandle_df3bfed7c6bd8d02(arg0) {
  const ret = arg0.createSyncAccessHandle()
  return ret
}

export function __wbg_crypto_86f2631e91b51511(arg0) {
  const ret = arg0.crypto
  return ret
}

export function __wbg_data_8bf4ae669a78a688(arg0) {
  const ret = arg0.data
  return ret
}

export function __wbg_dispatchEvent_50a40ea5c664f9f4() {
  return handleError(function (arg0, arg1) {
    const ret = arg0.dispatchEvent(arg1)
    return ret
  }, arguments)
}

export function __wbg_done_62ea16af4ce34b24(arg0) {
  const ret = arg0.done
  return ret
}

export function __wbg_enqueue_a7e6b1ee87963aad() {
  return handleError(function (arg0, arg1) {
    arg0.enqueue(arg1)
  }, arguments)
}

export function __wbg_entries_13da0847a5239578(arg0) {
  const ret = arg0.entries()
  return ret
}

export function __wbg_fetch_16dcf1cfbbc66b3c(arg0) {
  const ret = fetch(arg0)
  return ret
}

export function __wbg_fetch_6bbc32f991730587(arg0) {
  const ret = fetch(arg0)
  return ret
}

export function __wbg_fetch_90447c28cc0b095e(arg0, arg1) {
  const ret = arg0.fetch(arg1)
  return ret
}

export function __wbg_flush_554f5177ae6f76cb() {
  return handleError(function (arg0) {
    arg0.flush()
  }, arguments)
}

export function __wbg_getDirectory_9beed6c83b6861f5(arg0) {
  const ret = arg0.getDirectory()
  return ret
}

export function __wbg_getFileHandle_298ee7a4e5a85f84(arg0, arg1, arg2, arg3) {
  const ret = arg0.getFileHandle(getStringFromWasm0(arg1, arg2), arg3)
  return ret
}

export function __wbg_getRandomValues_1c61fac11405ffdc() {
  return handleError(function (arg0, arg1) {
    globalThis.crypto.getRandomValues(getArrayU8FromWasm0(arg0, arg1))
  }, arguments)
}

export function __wbg_getRandomValues_a8ddca022803a145() {
  return handleError(function (arg0, arg1) {
    globalThis.crypto.getRandomValues(getArrayU8FromWasm0(arg0, arg1))
  }, arguments)
}

export function __wbg_getRandomValues_b3f15fcbfabb0f8b() {
  return handleError(function (arg0, arg1) {
    arg0.getRandomValues(arg1)
  }, arguments)
}

export function __wbg_getReader_48e00749fe3f6089() {
  return handleError(function (arg0) {
    const ret = arg0.getReader()
    return ret
  }, arguments)
}

export function __wbg_getSize_1bf196c4094d8f7b() {
  return handleError(function (arg0) {
    const ret = arg0.getSize()
    return ret
  }, arguments)
}

export function __wbg_getTime_ad1e9878a735af08(arg0) {
  const ret = arg0.getTime()
  return ret
}

export function __wbg_get_6b7bd52aca3f9671(arg0, arg1) {
  const ret = arg0[arg1 >>> 0]
  return ret
}

export function __wbg_get_af9dab7e9603ea93() {
  return handleError(function (arg0, arg1) {
    const ret = Reflect.get(arg0, arg1)
    return ret
  }, arguments)
}

export function __wbg_get_done_f98a6e62c4e18fb9(arg0) {
  const ret = arg0.done
  return isLikeNone(ret) ? 0xffffff : ret ? 1 : 0
}

export function __wbg_get_value_63e39884ef11812e(arg0) {
  const ret = arg0.value
  return ret
}

export function __wbg_has_0e670569d65d3a45() {
  return handleError(function (arg0, arg1) {
    const ret = Reflect.has(arg0, arg1)
    return ret
  }, arguments)
}

export function __wbg_headers_654c30e1bcccc552(arg0) {
  const ret = arg0.headers
  return ret
}

export function __wbg_instanceof_ArrayBuffer_f3320d2419cd0355(arg0) {
  let result
  try {
    result = arg0 instanceof ArrayBuffer
  } catch (_) {
    result = false
  }
  const ret = result
  return ret
}

export function __wbg_instanceof_Blob_e9c51ce33a4b6181(arg0) {
  let result
  try {
    result = arg0 instanceof Blob
  } catch (_) {
    result = false
  }
  const ret = result
  return ret
}

export function __wbg_instanceof_Error_3443650560328fa9(arg0) {
  let result
  try {
    result = arg0 instanceof Error
  } catch (_) {
    result = false
  }
  const ret = result
  return ret
}

export function __wbg_instanceof_FileSystemDirectoryHandle_264085cadc86679a(
  arg0,
) {
  let result
  try {
    result = arg0 instanceof FileSystemDirectoryHandle
  } catch (_) {
    result = false
  }
  const ret = result
  return ret
}

export function __wbg_instanceof_FileSystemFileHandle_214d69e0ae063fc8(arg0) {
  let result
  try {
    result = arg0 instanceof FileSystemFileHandle
  } catch (_) {
    result = false
  }
  const ret = result
  return ret
}

export function __wbg_instanceof_FileSystemSyncAccessHandle_f9bee57f2517340b(
  arg0,
) {
  let result
  try {
    result = arg0 instanceof FileSystemSyncAccessHandle
  } catch (_) {
    result = false
  }
  const ret = result
  return ret
}

export function __wbg_instanceof_Response_cd74d1c2ac92cb0b(arg0) {
  let result
  try {
    result = arg0 instanceof Response
  } catch (_) {
    result = false
  }
  const ret = result
  return ret
}

export function __wbg_instanceof_WorkerGlobalScope_9a3411db21c65a54(arg0) {
  let result
  try {
    result = arg0 instanceof WorkerGlobalScope
  } catch (_) {
    result = false
  }
  const ret = result
  return ret
}

export function __wbg_isArray_51fd9e6422c0a395(arg0) {
  const ret = Array.isArray(arg0)
  return ret
}

export function __wbg_iterator_27b7c8b35ab3e86b() {
  const ret = Symbol.iterator
  return ret
}

export function __wbg_length_22ac23eaec9d8053(arg0) {
  const ret = arg0.length
  return ret
}

export function __wbg_message_0305fa7903f4b3d9(arg0) {
  const ret = arg0.message
  return ret
}

export function __wbg_message_a4e9a39ee8f92b17(arg0, arg1) {
  const ret = arg1.message
  const ptr1 = passStringToWasm0(
    ret,
    wasm.__wbindgen_malloc,
    wasm.__wbindgen_realloc,
  )
  const len1 = WASM_VECTOR_LEN
  getDataViewMemory0().setInt32(arg0 + 4 * 1, len1, true)
  getDataViewMemory0().setInt32(arg0 + 4 * 0, ptr1, true)
}

export function __wbg_msCrypto_d562bbe83e0d4b91(arg0) {
  const ret = arg0.msCrypto
  return ret
}

export function __wbg_name_f33243968228ce95(arg0) {
  const ret = arg0.name
  return ret
}

export function __wbg_navigator_11b7299bb7886507(arg0) {
  const ret = arg0.navigator
  return ret
}

export function __wbg_new_0_23cedd11d9b40c9d() {
  const ret = new Date()
  return ret
}

export function __wbg_new_1ba21ce319a06297() {
  const ret = new Object()
  return ret
}

export function __wbg_new_25f239778d6112b9() {
  const ret = new Array()
  return ret
}

export function __wbg_new_3c79b3bb1b32b7d3() {
  return handleError(function () {
    const ret = new Headers()
    return ret
  }, arguments)
}

export function __wbg_new_6421f6084cc5bc5a(arg0) {
  const ret = new Uint8Array(arg0)
  return ret
}

export function __wbg_new_7c30d1f874652e62() {
  return handleError(function (arg0, arg1) {
    const ret = new WebSocket(getStringFromWasm0(arg0, arg1))
    return ret
  }, arguments)
}

export function __wbg_new_881a222c65f168fc() {
  return handleError(function () {
    const ret = new AbortController()
    return ret
  }, arguments)
}

export function __wbg_new_df1173567d5ff028(arg0, arg1) {
  const ret = new Error(getStringFromWasm0(arg0, arg1))
  return ret
}

export function __wbg_new_ff12d2b041fb48f1(arg0, arg1) {
  try {
    var state0 = { a: arg0, b: arg1 }
    var cb0 = (arg0, arg1) => {
      const a = state0.a
      state0.a = 0
      try {
        return wasm_bindgen__convert__closures_____invoke__h9537e2360576635a(
          a,
          state0.b,
          arg0,
          arg1,
        )
      } finally {
        state0.a = a
      }
    }
    const ret = new Promise(cb0)
    return ret
  } finally {
    state0.a = state0.b = 0
  }
}

export function __wbg_new_from_slice_f9c22b9153b26992(arg0, arg1) {
  const ret = new Uint8Array(getArrayU8FromWasm0(arg0, arg1))
  return ret
}

export function __wbg_new_no_args_cb138f77cf6151ee(arg0, arg1) {
  const ret = new Function(getStringFromWasm0(arg0, arg1))
  return ret
}

export function __wbg_new_with_byte_offset_and_length_d85c3da1fd8df149(
  arg0,
  arg1,
  arg2,
) {
  const ret = new Uint8Array(arg0, arg1 >>> 0, arg2 >>> 0)
  return ret
}

export function __wbg_new_with_event_init_dict_8ce3ab55b0239ca3() {
  return handleError(function (arg0, arg1, arg2) {
    const ret = new CloseEvent(getStringFromWasm0(arg0, arg1), arg2)
    return ret
  }, arguments)
}

export function __wbg_new_with_length_aa5eaf41d35235e5(arg0) {
  const ret = new Uint8Array(arg0 >>> 0)
  return ret
}

export function __wbg_new_with_str_and_init_c5748f76f5108934() {
  return handleError(function (arg0, arg1, arg2) {
    const ret = new Request(getStringFromWasm0(arg0, arg1), arg2)
    return ret
  }, arguments)
}

export function __wbg_new_with_str_sequence_073466a4a5387941() {
  return handleError(function (arg0, arg1, arg2) {
    const ret = new WebSocket(getStringFromWasm0(arg0, arg1), arg2)
    return ret
  }, arguments)
}

export function __wbg_next_138a17bbf04e926c(arg0) {
  const ret = arg0.next
  return ret
}

export function __wbg_next_3cfe5c0fe2a4cc53() {
  return handleError(function (arg0) {
    const ret = arg0.next()
    return ret
  }, arguments)
}

export function __wbg_node_e1f24f89a7336c2e(arg0) {
  const ret = arg0.node
  return ret
}

export function __wbg_now_2c95c9de01293173(arg0) {
  const ret = arg0.now()
  return ret
}

export function __wbg_now_69d776cd24f5215b() {
  const ret = Date.now()
  return ret
}

export function __wbg_performance_7a3ffd0b17f663ad(arg0) {
  const ret = arg0.performance
  return ret
}

export function __wbg_process_3975fd6c72f520aa(arg0) {
  const ret = arg0.process
  return ret
}

export function __wbg_protocol_a74f36816d507cab(arg0, arg1) {
  const ret = arg1.protocol
  const ptr1 = passStringToWasm0(
    ret,
    wasm.__wbindgen_malloc,
    wasm.__wbindgen_realloc,
  )
  const len1 = WASM_VECTOR_LEN
  getDataViewMemory0().setInt32(arg0 + 4 * 1, len1, true)
  getDataViewMemory0().setInt32(arg0 + 4 * 0, ptr1, true)
}

export function __wbg_prototypesetcall_dfe9b766cdc1f1fd(arg0, arg1, arg2) {
  Uint8Array.prototype.set.call(getArrayU8FromWasm0(arg0, arg1), arg2)
}

export function __wbg_push_7d9be8f38fc13975(arg0, arg1) {
  const ret = arg0.push(arg1)
  return ret
}

export function __wbg_queueMicrotask_9b549dfce8865860(arg0) {
  const ret = arg0.queueMicrotask
  return ret
}

export function __wbg_queueMicrotask_fca69f5bfad613a5(arg0) {
  queueMicrotask(arg0)
}

export function __wbg_randomFillSync_f8c153b79f285817() {
  return handleError(function (arg0, arg1) {
    arg0.randomFillSync(arg1)
  }, arguments)
}

export function __wbg_read_0063be96fda4ddbb() {
  return handleError(function (arg0, arg1, arg2, arg3) {
    const ret = arg0.read(getArrayU8FromWasm0(arg1, arg2), arg3)
    return ret
  }, arguments)
}

export function __wbg_read_39c4b35efcd03c25(arg0) {
  const ret = arg0.read()
  return ret
}

export function __wbg_readyState_9d0976dcad561aa9(arg0) {
  const ret = arg0.readyState
  return ret
}

export function __wbg_reason_d4eb9e40592438c2(arg0, arg1) {
  const ret = arg1.reason
  const ptr1 = passStringToWasm0(
    ret,
    wasm.__wbindgen_malloc,
    wasm.__wbindgen_realloc,
  )
  const len1 = WASM_VECTOR_LEN
  getDataViewMemory0().setInt32(arg0 + 4 * 1, len1, true)
  getDataViewMemory0().setInt32(arg0 + 4 * 0, ptr1, true)
}

export function __wbg_releaseLock_a5912f590b185180(arg0) {
  arg0.releaseLock()
}

export function __wbg_removeEventListener_54bf92f4a849bd7d() {
  return handleError(function (arg0, arg1, arg2, arg3) {
    arg0.removeEventListener(getStringFromWasm0(arg1, arg2), arg3)
  }, arguments)
}

export function __wbg_removeEventListener_565e273024b68b75() {
  return handleError(function (arg0, arg1, arg2, arg3) {
    arg0.removeEventListener(getStringFromWasm0(arg1, arg2), arg3)
  }, arguments)
}

export function __wbg_require_b74f47fc2d022fd6() {
  return handleError(function () {
    const ret = module.require
    return ret
  }, arguments)
}

export function __wbg_resolve_fd5bfbaa4ce36e1e(arg0) {
  const ret = Promise.resolve(arg0)
  return ret
}

export function __wbg_respond_9f7fc54636c4a3af() {
  return handleError(function (arg0, arg1) {
    arg0.respond(arg1 >>> 0)
  }, arguments)
}

export function __wbg_send_7cc36bb628044281() {
  return handleError(function (arg0, arg1, arg2) {
    arg0.send(getStringFromWasm0(arg1, arg2))
  }, arguments)
}

export function __wbg_send_ea59e150ab5ebe08() {
  return handleError(function (arg0, arg1, arg2) {
    arg0.send(getArrayU8FromWasm0(arg1, arg2))
  }, arguments)
}

export function __wbg_setTimeout_2b111259203a2623() {
  return handleError(function (arg0, arg1, arg2) {
    const ret = arg0.setTimeout(arg1, arg2)
    return ret
  }, arguments)
}

export function __wbg_setTimeout_4eb823e8b72fbe79() {
  return handleError(function (arg0, arg1, arg2) {
    const ret = arg0.setTimeout(arg1, arg2)
    return ret
  }, arguments)
}

export function __wbg_setTimeout_4ec014681668a581(arg0, arg1) {
  const ret = setTimeout(arg0, arg1)
  return ret
}

export function __wbg_setTimeout_cb2a856ba8315e7a(arg0, arg1) {
  const ret = setTimeout(arg0, arg1)
  return ret
}

export function __wbg_setTimeout_eefe7f4c234b0c6b() {
  return handleError(function (arg0, arg1) {
    const ret = setTimeout(arg0, arg1)
    return ret
  }, arguments)
}

export function __wbg_set_169e13b608078b7b(arg0, arg1, arg2) {
  arg0.set(getArrayU8FromWasm0(arg1, arg2))
}

export function __wbg_set_at_8ed309b95b9da8e8(arg0, arg1) {
  arg0.at = arg1
}

export function __wbg_set_binaryType_73e8c75df97825f8(arg0, arg1) {
  arg0.binaryType = __wbindgen_enum_BinaryType[arg1]
}

export function __wbg_set_body_8e743242d6076a4f(arg0, arg1) {
  arg0.body = arg1
}

export function __wbg_set_cache_0e437c7c8e838b9b(arg0, arg1) {
  arg0.cache = __wbindgen_enum_RequestCache[arg1]
}

export function __wbg_set_code_2f1b419c1a6169a3(arg0, arg1) {
  arg0.code = arg1
}

export function __wbg_set_create_c87a4965b38c1564(arg0, arg1) {
  arg0.create = arg1 !== 0
}

export function __wbg_set_credentials_55ae7c3c106fd5be(arg0, arg1) {
  arg0.credentials = __wbindgen_enum_RequestCredentials[arg1]
}

export function __wbg_set_handle_event_14baa3949ef6909d(arg0, arg1) {
  arg0.handleEvent = arg1
}

export function __wbg_set_headers_5671cf088e114d2b(arg0, arg1) {
  arg0.headers = arg1
}

export function __wbg_set_method_76c69e41b3570627(arg0, arg1, arg2) {
  arg0.method = getStringFromWasm0(arg1, arg2)
}

export function __wbg_set_mode_611016a6818fc690(arg0, arg1) {
  arg0.mode = __wbindgen_enum_RequestMode[arg1]
}

export function __wbg_set_once_cb88c6a887803dfa(arg0, arg1) {
  arg0.once = arg1 !== 0
}

export function __wbg_set_onclose_032729b3d7ed7a9e(arg0, arg1) {
  arg0.onclose = arg1
}

export function __wbg_set_onerror_7819daa6af176ddb(arg0, arg1) {
  arg0.onerror = arg1
}

export function __wbg_set_onmessage_71321d0bed69856c(arg0, arg1) {
  arg0.onmessage = arg1
}

export function __wbg_set_onopen_6d4abedb27ba5656(arg0, arg1) {
  arg0.onopen = arg1
}

export function __wbg_set_reason_6cb672258b901b3a(arg0, arg1, arg2) {
  arg0.reason = getStringFromWasm0(arg1, arg2)
}

export function __wbg_set_signal_e89be862d0091009(arg0, arg1) {
  arg0.signal = arg1
}

export function __wbg_signal_3c14fbdc89694b39(arg0) {
  const ret = arg0.signal
  return ret
}

export function __wbg_static_accessor_GLOBAL_769e6b65d6557335() {
  const ret = typeof global === 'undefined' ? null : global
  return isLikeNone(ret) ? 0 : addToExternrefTable0(ret)
}

export function __wbg_static_accessor_GLOBAL_THIS_60cf02db4de8e1c1() {
  const ret = typeof globalThis === 'undefined' ? null : globalThis
  return isLikeNone(ret) ? 0 : addToExternrefTable0(ret)
}

export function __wbg_static_accessor_SELF_08f5a74c69739274() {
  const ret = typeof self === 'undefined' ? null : self
  return isLikeNone(ret) ? 0 : addToExternrefTable0(ret)
}

export function __wbg_static_accessor_WINDOW_a8924b26aa92d024() {
  const ret = typeof window === 'undefined' ? null : window
  return isLikeNone(ret) ? 0 : addToExternrefTable0(ret)
}

export function __wbg_status_9bfc680efca4bdfd(arg0) {
  const ret = arg0.status
  return ret
}

export function __wbg_storage_7db24ea4f9f4aa79(arg0) {
  const ret = arg0.storage
  return ret
}

export function __wbg_stringify_655a6390e1f5eb6b() {
  return handleError(function (arg0) {
    const ret = JSON.stringify(arg0)
    return ret
  }, arguments)
}

export function __wbg_subarray_845f2f5bce7d061a(arg0, arg1, arg2) {
  const ret = arg0.subarray(arg1 >>> 0, arg2 >>> 0)
  return ret
}

export function __wbg_text_51046bb33d257f63() {
  return handleError(function (arg0) {
    const ret = arg0.text()
    return ret
  }, arguments)
}

export function __wbg_then_429f7caf1026411d(arg0, arg1, arg2) {
  const ret = arg0.then(arg1, arg2)
  return ret
}

export function __wbg_then_4f95312d68691235(arg0, arg1) {
  const ret = arg0.then(arg1)
  return ret
}

export function __wbg_toString_14b47ee7542a49ef(arg0) {
  const ret = arg0.toString()
  return ret
}

export function __wbg_truncate_07b2629f3dbd9443() {
  return handleError(function (arg0, arg1) {
    arg0.truncate(arg1)
  }, arguments)
}

export function __wbg_url_b6d11838a4f95198(arg0, arg1) {
  const ret = arg1.url
  const ptr1 = passStringToWasm0(
    ret,
    wasm.__wbindgen_malloc,
    wasm.__wbindgen_realloc,
  )
  const len1 = WASM_VECTOR_LEN
  getDataViewMemory0().setInt32(arg0 + 4 * 1, len1, true)
  getDataViewMemory0().setInt32(arg0 + 4 * 0, ptr1, true)
}

export function __wbg_url_df28eef824b04410(arg0, arg1) {
  const ret = arg1.url
  const ptr1 = passStringToWasm0(
    ret,
    wasm.__wbindgen_malloc,
    wasm.__wbindgen_realloc,
  )
  const len1 = WASM_VECTOR_LEN
  getDataViewMemory0().setInt32(arg0 + 4 * 1, len1, true)
  getDataViewMemory0().setInt32(arg0 + 4 * 0, ptr1, true)
}

export function __wbg_value_57b7b035e117f7ee(arg0) {
  const ret = arg0.value
  return ret
}

export function __wbg_versions_4e31226f5e8dc909(arg0) {
  const ret = arg0.versions
  return ret
}

export function __wbg_view_788aaf149deefd2f(arg0) {
  const ret = arg0.view
  return isLikeNone(ret) ? 0 : addToExternrefTable0(ret)
}

export function __wbg_wasClean_4154a2d59fdb4dd7(arg0) {
  const ret = arg0.wasClean
  return ret
}

export function __wbg_write_f87f327ea3e1dd4b() {
  return handleError(function (arg0, arg1, arg2, arg3) {
    const ret = arg0.write(getArrayU8FromWasm0(arg1, arg2), arg3)
    return ret
  }, arguments)
}

export function __wbindgen_cast_2241b6af4c4b2941(arg0, arg1) {
  // Cast intrinsic for `Ref(String) -> Externref`.
  const ret = getStringFromWasm0(arg0, arg1)
  return ret
}

export function __wbindgen_cast_3ea3039d54b373eb(arg0, arg1) {
  // Cast intrinsic for `Closure(Closure { dtor_idx: 10272, function: Function { arguments: [NamedExternref("Event")], shim_idx: 10273, ret: Unit, inner_ret: Some(Unit) }, mutable: true }) -> Externref`.
  const ret = makeMutClosure(
    arg0,
    arg1,
    wasm.wasm_bindgen__closure__destroy__h50e54ca2eace6155,
    wasm_bindgen__convert__closures_____invoke__h00dbb47cb783c1cb,
  )
  return ret
}

export function __wbindgen_cast_4153b1f55d6c8fdf(arg0, arg1) {
  // Cast intrinsic for `Closure(Closure { dtor_idx: 12672, function: Function { arguments: [NamedExternref("MessageEvent")], shim_idx: 12673, ret: Unit, inner_ret: Some(Unit) }, mutable: true }) -> Externref`.
  const ret = makeMutClosure(
    arg0,
    arg1,
    wasm.wasm_bindgen__closure__destroy__h0c968a989bd6a6a9,
    wasm_bindgen__convert__closures_____invoke__h3e3bf5ab6c41937b,
  )
  return ret
}

export function __wbindgen_cast_4e858f5f0703a96a(arg0, arg1) {
  // Cast intrinsic for `Closure(Closure { dtor_idx: 9435, function: Function { arguments: [], shim_idx: 9436, ret: Unit, inner_ret: Some(Unit) }, mutable: false }) -> Externref`.
  const ret = makeClosure(
    arg0,
    arg1,
    wasm.wasm_bindgen__closure__destroy__hdff13065a173c70e,
    wasm_bindgen__convert__closures_____invoke__h9255eddd6f35e8b7,
  )
  return ret
}

export function __wbindgen_cast_7cd11cc038cab985(arg0, arg1) {
  // Cast intrinsic for `Closure(Closure { dtor_idx: 9293, function: Function { arguments: [], shim_idx: 9294, ret: Unit, inner_ret: Some(Unit) }, mutable: true }) -> Externref`.
  const ret = makeMutClosure(
    arg0,
    arg1,
    wasm.wasm_bindgen__closure__destroy__hcc7ac7ba090ca9ea,
    wasm_bindgen__convert__closures_____invoke__h07593877f7995c23,
  )
  return ret
}

export function __wbindgen_cast_887271372336c124(arg0, arg1) {
  // Cast intrinsic for `Closure(Closure { dtor_idx: 10272, function: Function { arguments: [NamedExternref("CloseEvent")], shim_idx: 10273, ret: Unit, inner_ret: Some(Unit) }, mutable: true }) -> Externref`.
  const ret = makeMutClosure(
    arg0,
    arg1,
    wasm.wasm_bindgen__closure__destroy__h50e54ca2eace6155,
    wasm_bindgen__convert__closures_____invoke__h00dbb47cb783c1cb,
  )
  return ret
}

export function __wbindgen_cast_9352dd3de84c8019(arg0, arg1) {
  // Cast intrinsic for `Closure(Closure { dtor_idx: 12851, function: Function { arguments: [], shim_idx: 12852, ret: Unit, inner_ret: Some(Unit) }, mutable: true }) -> Externref`.
  const ret = makeMutClosure(
    arg0,
    arg1,
    wasm.wasm_bindgen__closure__destroy__hebb8cfb02927fdfc,
    wasm_bindgen__convert__closures_____invoke__hed3a942a68768433,
  )
  return ret
}

export function __wbindgen_cast_a50a00657457b93c(arg0, arg1) {
  // Cast intrinsic for `Closure(Closure { dtor_idx: 10272, function: Function { arguments: [], shim_idx: 10274, ret: Unit, inner_ret: Some(Unit) }, mutable: true }) -> Externref`.
  const ret = makeMutClosure(
    arg0,
    arg1,
    wasm.wasm_bindgen__closure__destroy__h50e54ca2eace6155,
    wasm_bindgen__convert__closures_____invoke__hebe6e299b2388f3c,
  )
  return ret
}

export function __wbindgen_cast_aeebdcea3e932e61(arg0, arg1) {
  // Cast intrinsic for `Closure(Closure { dtor_idx: 11586, function: Function { arguments: [], shim_idx: 11587, ret: Unit, inner_ret: Some(Unit) }, mutable: false }) -> Externref`.
  const ret = makeClosure(
    arg0,
    arg1,
    wasm.wasm_bindgen__closure__destroy__hce14e60eccb28ced,
    wasm_bindgen__convert__closures_____invoke__hac25bb55b8468f62,
  )
  return ret
}

export function __wbindgen_cast_b3db8d5ad7f5a0a1(arg0, arg1) {
  // Cast intrinsic for `Closure(Closure { dtor_idx: 10950, function: Function { arguments: [NamedExternref("CloseEvent")], shim_idx: 10951, ret: Unit, inner_ret: Some(Unit) }, mutable: true }) -> Externref`.
  const ret = makeMutClosure(
    arg0,
    arg1,
    wasm.wasm_bindgen__closure__destroy__h4397227f39881d0f,
    wasm_bindgen__convert__closures_____invoke__h732e77c759a01739,
  )
  return ret
}

export function __wbindgen_cast_c7cf0ff0665b4649(arg0, arg1) {
  // Cast intrinsic for `Closure(Closure { dtor_idx: 10170, function: Function { arguments: [], shim_idx: 10171, ret: Unit, inner_ret: Some(Unit) }, mutable: true }) -> Externref`.
  const ret = makeMutClosure(
    arg0,
    arg1,
    wasm.wasm_bindgen__closure__destroy__h3859cf66281a9e68,
    wasm_bindgen__convert__closures_____invoke__h81332cdc358993a9,
  )
  return ret
}

export function __wbindgen_cast_cb9088102bce6b30(arg0, arg1) {
  // Cast intrinsic for `Ref(Slice(U8)) -> NamedExternref("Uint8Array")`.
  const ret = getArrayU8FromWasm0(arg0, arg1)
  return ret
}

export function __wbindgen_cast_d2f2b2bfd4068b8a(arg0, arg1) {
  // Cast intrinsic for `Closure(Closure { dtor_idx: 10272, function: Function { arguments: [NamedExternref("MessageEvent")], shim_idx: 10273, ret: Unit, inner_ret: Some(Unit) }, mutable: true }) -> Externref`.
  const ret = makeMutClosure(
    arg0,
    arg1,
    wasm.wasm_bindgen__closure__destroy__h50e54ca2eace6155,
    wasm_bindgen__convert__closures_____invoke__h00dbb47cb783c1cb,
  )
  return ret
}

export function __wbindgen_cast_e9f9353b25879219(arg0, arg1) {
  // Cast intrinsic for `Closure(Closure { dtor_idx: 12835, function: Function { arguments: [Externref], shim_idx: 12836, ret: Unit, inner_ret: Some(Unit) }, mutable: true }) -> Externref`.
  const ret = makeMutClosure(
    arg0,
    arg1,
    wasm.wasm_bindgen__closure__destroy__hb6f071d46a71094a,
    wasm_bindgen__convert__closures_____invoke__h637d0d38d8ce5792,
  )
  return ret
}

export function __wbindgen_cast_ed0207ccdc4658c2(arg0, arg1) {
  // Cast intrinsic for `Closure(Closure { dtor_idx: 8143, function: Function { arguments: [NamedExternref("CloseEvent")], shim_idx: 8144, ret: Unit, inner_ret: Some(Unit) }, mutable: true }) -> Externref`.
  const ret = makeMutClosure(
    arg0,
    arg1,
    wasm.wasm_bindgen__closure__destroy__h0f59e50fd72a3fdd,
    wasm_bindgen__convert__closures_____invoke__hcbcb149b9e49fb4e,
  )
  return ret
}

export function __wbindgen_cast_fec6680cb41e645a(arg0, arg1) {
  // Cast intrinsic for `Closure(Closure { dtor_idx: 11704, function: Function { arguments: [], shim_idx: 11705, ret: Unit, inner_ret: Some(Unit) }, mutable: true }) -> Externref`.
  const ret = makeMutClosure(
    arg0,
    arg1,
    wasm.wasm_bindgen__closure__destroy__hd2524fd0114211dd,
    wasm_bindgen__convert__closures_____invoke__hfec9849f3ac1bcdf,
  )
  return ret
}

export function __wbindgen_cast_fedc9b51ee3c236a(arg0, arg1) {
  // Cast intrinsic for `Closure(Closure { dtor_idx: 12705, function: Function { arguments: [], shim_idx: 12706, ret: Unit, inner_ret: Some(Unit) }, mutable: true }) -> Externref`.
  const ret = makeMutClosure(
    arg0,
    arg1,
    wasm.wasm_bindgen__closure__destroy__haace06bd8f1e7578,
    wasm_bindgen__convert__closures_____invoke__hb5c66d63acb43222,
  )
  return ret
}

export function __wbindgen_init_externref_table() {
  const table = wasm.__wbindgen_externrefs
  const offset = table.grow(4)
  table.set(0, undefined)
  table.set(offset + 0, undefined)
  table.set(offset + 1, null)
  table.set(offset + 2, true)
  table.set(offset + 3, false)
}
