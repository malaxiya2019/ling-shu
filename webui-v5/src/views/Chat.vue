<script setup lang="ts">
import { ref, nextTick } from 'vue'
import { sendChat } from '@/api'
import type { ChatMessage } from '@/api/types'

const messages = ref<ChatMessage[]>([])
const input = ref('')
const loading = ref(false)
const error = ref('')
const chatContainer = ref<HTMLElement | null>(null)

async function send() {
  if (!input.value.trim() || loading.value) return

  const text = input.value
  input.value = ''

  // Add user message
  messages.value.push({ role: 'user', content: text })
  await scrollToBottom()

  loading.value = true
  error.value = ''

  try {
    const resp = await sendChat({
      messages: messages.value,
      stream: false,
    })
    messages.value.push(resp.message)
  } catch (e: any) {
    error.value = e?.message || '发送失败'
    // Re-enable input
  }

  loading.value = false
  await scrollToBottom()
}

async function scrollToBottom() {
  await nextTick()
  if (chatContainer.value) {
    chatContainer.value.scrollTop = chatContainer.value.scrollHeight
  }
}

function newChat() {
  messages.value = []
  error.value = ''
}
</script>

<template>
  <div class="chat-page">
    <header class="chat-header">
      <h1 class="chat-title">💬 AI 助手</h1>
      <div class="header-actions">
        <button class="header-btn" @click="newChat" title="新建对话">＋ 新对话</button>
      </div>
    </header>

    <div class="messages" ref="chatContainer">
      <div v-if="messages.length === 0 && !loading" class="empty-state">
        <div class="empty-icon">💬</div>
        <h2>开始对话</h2>
        <p>与灵枢 AI 助手交流，获取智能帮助</p>
        <div class="suggestions">
          <button class="suggestion-chip" @click="input='解释一下什么是 AI Agent'">解释一下什么是 AI Agent</button>
          <button class="suggestion-chip" @click="input='用 Python 写一个快速排序'">用 Python 写一个快速排序</button>
          <button class="suggestion-chip" @click="input='帮我总结这段文本'">帮我总结这段文本</button>
        </div>
      </div>

      <div v-for="(msg, i) in messages" :key="i" :class="['msg', msg.role]">
        <div class="msg-avatar">{{ msg.role === 'user' ? '🧑' : '🤖' }}</div>
        <div class="msg-bubble">{{ msg.content }}</div>
      </div>

      <div v-if="loading" class="msg assistant">
        <div class="msg-avatar">🤖</div>
        <div class="msg-bubble typing">
          <span class="dot"></span><span class="dot"></span><span class="dot"></span>
        </div>
      </div>

      <div v-if="error" class="error-msg">
        ⚠️ {{ error }}
      </div>
    </div>

    <div class="input-bar">
      <input
        v-model="input"
        type="text"
        placeholder="输入消息…"
        @keydown.enter="send"
        :disabled="loading"
      />
      <button @click="send" :disabled="loading || !input.trim()" class="send-btn">
        ➤
      </button>
    </div>
  </div>
</template>

<style scoped>
.chat-page {
  display: flex;
  flex-direction: column;
  height: 100dvh;
  padding-bottom: var(--nav-height);
}

.chat-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 12px 16px;
  border-bottom: 1px solid var(--border);
  background: var(--bg-primary);
  flex-shrink: 0;
}

.chat-title {
  font-size: 1rem;
  font-weight: 700;
}

.header-btn {
  background: none;
  border: 1px solid var(--border);
  border-radius: 8px;
  padding: 6px 12px;
  color: var(--text-secondary);
  font-size: 0.8rem;
  cursor: pointer;
  font-family: inherit;
  transition: border-color 0.15s;
}

.header-btn:hover { border-color: var(--accent-blue); color: var(--accent-blue); }

.messages {
  flex: 1;
  overflow-y: auto;
  padding: 16px;
  display: flex;
  flex-direction: column;
  gap: 16px;
}

.msg {
  display: flex;
  gap: 10px;
  max-width: 88%;
  animation: fadeIn 0.2s ease;
}

@keyframes fadeIn {
  from { opacity: 0; transform: translateY(8px); }
  to { opacity: 1; transform: translateY(0); }
}

.msg.user { align-self: flex-end; flex-direction: row-reverse; }

.msg-avatar {
  font-size: 1.3rem;
  flex-shrink: 0;
}

.msg-bubble {
  padding: 10px 14px;
  border-radius: var(--radius);
  font-size: 0.88rem;
  line-height: 1.6;
  color: var(--text-primary);
  white-space: pre-wrap;
  word-break: break-word;
}

.msg.user .msg-bubble {
  background: var(--accent-blue);
  color: #fff;
  border-bottom-right-radius: 4px;
}

.msg.assistant .msg-bubble {
  background: var(--bg-tertiary);
  border: 1px solid var(--border);
  border-bottom-left-radius: 4px;
}

.empty-state {
  flex: 1;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 8px;
  color: var(--text-muted);
  padding: 0 16px;
}

.empty-icon { font-size: 3rem; margin-bottom: 8px; }
.empty-state h2 { font-size: 1.1rem; color: var(--text-secondary); }
.empty-state p { font-size: 0.85rem; }

.suggestions {
  display: flex;
  flex-direction: column;
  gap: 8px;
  margin-top: 20px;
  width: 100%;
  max-width: 320px;
}

.suggestion-chip {
  background: var(--bg-secondary);
  border: 1px solid var(--border);
  border-radius: var(--radius);
  padding: 10px 14px;
  color: var(--text-secondary);
  font-size: 0.82rem;
  cursor: pointer;
  transition: border-color 0.15s;
  text-align: center;
  font-family: inherit;
  width: 100%;
}

.suggestion-chip:hover {
  border-color: var(--accent-blue);
  color: var(--accent-blue);
}

.typing {
  display: flex;
  gap: 4px;
  align-items: center;
  padding: 10px 14px;
}

.dot {
  width: 6px;
  height: 6px;
  border-radius: 50%;
  background: var(--text-muted);
  animation: bounce 1.4s ease-in-out infinite both;
}

.dot:nth-child(1) { animation-delay: -0.32s; }
.dot:nth-child(2) { animation-delay: -0.16s; }
.dot:nth-child(3) { animation-delay: 0s; }

@keyframes bounce {
  0%, 80%, 100% { transform: scale(0); }
  40% { transform: scale(1); }
}

.error-msg {
  text-align: center;
  color: var(--accent-red);
  font-size: 0.8rem;
  padding: 8px;
}

.input-bar {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 10px 16px;
  border-top: 1px solid var(--border);
  background: var(--bg-primary);
  flex-shrink: 0;
}

.input-bar input {
  flex: 1;
  background: var(--bg-tertiary);
  border: 1px solid var(--border);
  border-radius: 20px;
  padding: 10px 16px;
  color: var(--text-primary);
  font-size: 0.9rem;
  outline: none;
  transition: border-color 0.15s;
  font-family: inherit;
}

.input-bar input:focus { border-color: var(--accent-blue); }
.input-bar input::placeholder { color: var(--text-muted); }

.send-btn {
  width: 40px;
  height: 40px;
  border-radius: 50%;
  background: var(--accent-blue);
  border: none;
  color: #fff;
  font-size: 1.1rem;
  cursor: pointer;
  display: flex;
  align-items: center;
  justify-content: center;
  transition: opacity 0.15s, transform 0.1s;
  flex-shrink: 0;
}

.send-btn:active { transform: scale(0.9); }
.send-btn:disabled { opacity: 0.4; cursor: not-allowed; }
</style>
