# Операции антагонистов (BandaStation)
Модпак `antag_operations`: сценарии-«операции», связывающие нескольких антагонистов общими личными целями.

### Каркас операций
`/datum/antag_operation` владеет участниками и их личными целями, отслеживает удаление, уход в крио и снятие роли участника и отменяет операцию с уведомлением.

- **Код:** [`/datum/antag_operation`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/antag_operations/code/antag_operation.dm#L5), [`/datum/antag_operation/proc/cancel()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/antag_operations/code/antag_operation.dm#L77)
- **BandaStation:** [`_antag_operations.dm`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/antag_operations/_antag_operations.dm#L1)
- **Статус в ss13-rs:** ❌ нет

### Обмен документами
В начале раунда двум обычным предателям выдаётся операция: у каждого оригинал секретных документов (в руках или в запертом чемодане в техтоннелях с целеуказателем и кодом), цель — заполучить оригинал партнёра и сохранить свой; возможны условия предательства.

- **Код:** [`/datum/antag_operation/document_exchange`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/antag_operations/code/document_exchange.dm#L6), [`/datum/objective/document_exchange`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/antag_operations/code/document_exchange.dm#L132), [`try_document_exchange()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/antag_operations/code/document_exchange_assignment.dm#L11)
- **Статус в ss13-rs:** ❌ нет

### Админская выдача обмена
Админ может вручную создать обмен для выбранных агентов с выбором условий и способа доставки.

- **Код:** [`/datum/antag_operation/document_exchange/proc/admin_start()`](https://github.com/ss220club/BandaStation/blob/ce5b1f159748db35281bd3b673551598807f4a0b/modular_bandastation/antag_operations/code/document_exchange_admin.dm#L7)
- **Статус в ss13-rs:** ❌ нет
