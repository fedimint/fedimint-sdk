<template>
  <div class="full-width-layout">
    <Content />
  </div>
</template>

<style scoped>
.full-width-layout {
  max-width: 100% !important;
  padding: 0 32px;
}

.full-width-layout :deep(.container) {
  max-width: none;
}

@media (max-width: 768px) {
  .full-width-layout {
    padding: 0 16px;
  }
}
</style>
