/*
 * usbtransfer.c
 *
 * USB transfer completion routines, continuous reader callbacks,
 * ring buffer implementation, and zero-length packet (ZLP) handling.
 *
 * Equivalent to the original Data_InTransReorder, Data_InTransComplete,
 * Data_OutTransComplete, and ring buffer functions (sections 9–10).
 *
 * SPDX-License-Identifier: MIT
 */

#include "mtk_usb2ser.h"

/* =========================================================================
 *  Ring Buffer Implementation
 * ========================================================================= */

/* -------------------------------------------------------------------------
 *  RingBufferInit — allocate and initialise a ring buffer.
 * ------------------------------------------------------------------------- */
NTSTATUS
RingBufferInit(
    _Inout_ PRING_BUFFER Ring,
    _In_    ULONG        Size
    )
{
    Ring->Buffer = (PUCHAR)ExAllocatePool2(POOL_FLAG_NON_PAGED, Size, 'RBSM');
    if (Ring->Buffer == NULL) {
        return STATUS_INSUFFICIENT_RESOURCES;
    }

    Ring->Size       = Size;
    Ring->Head       = 0;
    Ring->Tail       = 0;
    Ring->DataLength = 0;

    return STATUS_SUCCESS;
}

/* -------------------------------------------------------------------------
 *  RingBufferFree — release ring buffer memory.
 * ------------------------------------------------------------------------- */
VOID
RingBufferFree(
    _Inout_ PRING_BUFFER Ring
    )
{
    if (Ring->Buffer != NULL) {
        ExFreePoolWithTag(Ring->Buffer, 'RBSM');
        Ring->Buffer     = NULL;
        Ring->Size       = 0;
        Ring->Head       = 0;
        Ring->Tail       = 0;
        Ring->DataLength = 0;
    }
}

/* -------------------------------------------------------------------------
 *  RingBufferWrite — append data into the ring buffer.
 *  Returns the number of bytes actually written.
 *  Caller must hold Ring->Lock if accessed from multiple contexts.
 * ------------------------------------------------------------------------- */
ULONG
RingBufferWrite(
    _Inout_ PRING_BUFFER    Ring,
    _In_reads_(Length) PUCHAR Data,
    _In_    ULONG           Length
    )
{
    ULONG available;
    ULONG written = 0;
    ULONG chunk;

    if (Ring->Buffer == NULL || Length == 0) {
        return 0;
    }

    WdfSpinLockAcquire(Ring->Lock);

    available = Ring->Size - Ring->DataLength;
    if (Length > available) {
        Length = available;
    }

    written = Length;

    /* First chunk: Head to end of buffer */
    chunk = Ring->Size - Ring->Head;
    if (chunk > Length) {
        chunk = Length;
    }

    RtlCopyMemory(Ring->Buffer + Ring->Head, Data, chunk);
    Ring->Head = (Ring->Head + chunk) % Ring->Size;
    Length -= chunk;

    /* Second chunk: wrap around to start of buffer */
    if (Length > 0) {
        RtlCopyMemory(Ring->Buffer + Ring->Head, Data + chunk, Length);
        Ring->Head = (Ring->Head + Length) % Ring->Size;
    }

    Ring->DataLength += written;

    WdfSpinLockRelease(Ring->Lock);

    return written;
}

/* -------------------------------------------------------------------------
 *  RingBufferRead — consume data from the ring buffer.
 *  Returns the number of bytes actually read.
 *  Caller must hold Ring->Lock if accessed from multiple contexts.
 * ------------------------------------------------------------------------- */
ULONG
RingBufferRead(
    _Inout_ PRING_BUFFER    Ring,
    _Out_writes_(Length) PUCHAR Data,
    _In_    ULONG           Length
    )
{
    ULONG toRead;
    ULONG chunk;
    ULONG totalRead;

    if (Ring->Buffer == NULL || Ring->DataLength == 0) {
        return 0;
    }

    WdfSpinLockAcquire(Ring->Lock);

    toRead = Ring->DataLength;
    if (Length < toRead) {
        toRead = Length;
    }

    totalRead = toRead;

    /* First chunk: Tail to end of buffer */
    chunk = Ring->Size - Ring->Tail;
    if (chunk > toRead) {
        chunk = toRead;
    }

    RtlCopyMemory(Data, Ring->Buffer + Ring->Tail, chunk);
    Ring->Tail = (Ring->Tail + chunk) % Ring->Size;
    toRead -= chunk;

    /* Second chunk: wrap around */
    if (toRead > 0) {
        RtlCopyMemory(Data + chunk, Ring->Buffer + Ring->Tail, toRead);
        Ring->Tail = (Ring->Tail + toRead) % Ring->Size;
    }

    Ring->DataLength -= totalRead;

    WdfSpinLockRelease(Ring->Lock);

    return totalRead;
}

/* -------------------------------------------------------------------------
 *  RingBufferGetDataLength — return current data count.
 * ------------------------------------------------------------------------- */
ULONG
RingBufferGetDataLength(
    _In_ PRING_BUFFER Ring
    )
{
    ULONG len;

    WdfSpinLockAcquire(Ring->Lock);
    len = Ring->DataLength;
    WdfSpinLockRelease(Ring->Lock);

    return len;
}

/* -------------------------------------------------------------------------
 *  RingBufferPurge — discard all data in the ring buffer.
 * ------------------------------------------------------------------------- */
VOID
RingBufferPurge(
    _Inout_ PRING_BUFFER Ring
    )
{
    WdfSpinLockAcquire(Ring->Lock);
    Ring->Head       = 0;
    Ring->Tail       = 0;
    Ring->DataLength = 0;
    WdfSpinLockRelease(Ring->Lock);
}

/* =========================================================================
 *  EvtUsbBulkInReadComplete — continuous reader callback for Bulk IN
 *
 *  Called by KMDF each time data arrives from the device on the bulk IN
 *  pipe. Appends the data to the ring buffer and attempts to complete
 *  any pending read requests.
 *
 *  Equivalent to the original Data_InTransComplete / Data_InTransReorder.
 * ========================================================================= */
VOID
EvtUsbBulkInReadComplete(
    _In_ WDFUSBPIPE Pipe,
    _In_ WDFMEMORY  Buffer,
    _In_ size_t     NumBytesTransferred,
    _In_ WDFCONTEXT Context
    )
{
    PDEVICE_CONTEXT devCtx = (PDEVICE_CONTEXT)Context;
    PUCHAR          data;
    ULONG           bytesWritten;

    UNREFERENCED_PARAMETER(Pipe);

    if (NumBytesTransferred == 0) {
        return;
    }

    data = (PUCHAR)WdfMemoryGetBuffer(Buffer, NULL);

    /* Append received data to ring buffer (locking handled internally) */
    bytesWritten = RingBufferWrite(
        &devCtx->ReadBuffer,
        data,
        (ULONG)NumBytesTransferred
        );

    if (bytesWritten < (ULONG)NumBytesTransferred) {
        /* Buffer overflow — track overrun errors */
        devCtx->PerfStats.BufferOverrunErrorCount++;
    }

    /* Signal SERIAL_EV_RXCHAR event for any pending wait-on-mask */
    SerialCompleteWaitOnMask(devCtx, SERIAL_EV_RXCHAR);

    /* Try to satisfy pending read requests from the ring buffer */
    {
        WDFREQUEST  pendingRead;
        NTSTATUS    status;

        while (NT_SUCCESS(
            WdfIoQueueRetrieveNextRequest(
                devCtx->PendingReadQueue,
                &pendingRead))) {

            PVOID   readBuf;
            size_t  readLen;
            ULONG   bytesRead;

            status = WdfRequestRetrieveOutputBuffer(
                pendingRead, 1, &readBuf, &readLen);

            if (NT_SUCCESS(status)) {
                bytesRead = RingBufferRead(
                    &devCtx->ReadBuffer,
                    (PUCHAR)readBuf,
                    (ULONG)readLen
                    );

                if (bytesRead > 0) {
                    devCtx->PerfStats.ReceivedCount += bytesRead;
                    WdfRequestCompleteWithInformation(
                        pendingRead, STATUS_SUCCESS, bytesRead);
                } else {
                    /*
                     * No more data — re-queue the request.
                     * If re-queuing fails, complete with error.
                     */
                    status = WdfRequestForwardToIoQueue(
                        pendingRead, devCtx->PendingReadQueue);
                    if (!NT_SUCCESS(status)) {
                        WdfRequestComplete(pendingRead, status);
                    }
                    break;
                }
            } else {
                WdfRequestComplete(pendingRead, status);
            }
        }
    }
}

/* =========================================================================
 *  EvtUsbBulkInReadersFailed — error handler for Bulk IN continuous reader
 *
 *  Returning TRUE tells KMDF to reset the pipe and restart the reader.
 *  Returning FALSE would stop the reader permanently.
 * ========================================================================= */
BOOLEAN
EvtUsbBulkInReadersFailed(
    _In_ WDFUSBPIPE    Pipe,
    _In_ NTSTATUS      Status,
    _In_ USBD_STATUS   UsbdStatus
    )
{
    UNREFERENCED_PARAMETER(Pipe);
    UNREFERENCED_PARAMETER(Status);
    UNREFERENCED_PARAMETER(UsbdStatus);

    /* Reset the pipe and restart the continuous reader */
    return TRUE;
}

/* =========================================================================
 *  EvtUsbInterruptReadComplete — continuous reader callback for Interrupt IN
 *
 *  Processes CDC ACM Serial State notifications.
 *  These carry modem status bits (DCD, DSR, RI, etc.) and error flags.
 *
 *  Notification format:
 *    CDC_NOTIFICATION header (8 bytes) + USHORT serialState
 * ========================================================================= */
VOID
EvtUsbInterruptReadComplete(
    _In_ WDFUSBPIPE Pipe,
    _In_ WDFMEMORY  Buffer,
    _In_ size_t     NumBytesTransferred,
    _In_ WDFCONTEXT Context
    )
{
    PDEVICE_CONTEXT     devCtx = (PDEVICE_CONTEXT)Context;
    PUCHAR              data;
    PCDC_NOTIFICATION   notification;
    USHORT              serialState;
    ULONG               newModemStatus;
    ULONG               events = 0;

    UNREFERENCED_PARAMETER(Pipe);

    /* Need at least the notification header + 2 bytes of data */
    if (NumBytesTransferred < sizeof(CDC_NOTIFICATION) + sizeof(USHORT)) {
        return;
    }

    data = (PUCHAR)WdfMemoryGetBuffer(Buffer, NULL);
    notification = (PCDC_NOTIFICATION)data;

    /* Only process SERIAL_STATE notifications */
    if (notification->bNotification != CDC_NOTIFICATION_SERIAL_STATE) {
        return;
    }

    serialState = *(PUSHORT)(data + sizeof(CDC_NOTIFICATION));

    /*
     * Map CDC serial state bits to Windows modem status register bits.
     * The modem status register format matches MS_*_ON definitions.
     */
    newModemStatus = 0;

    if (serialState & CDC_SERIAL_STATE_DCD) {
        newModemStatus |= SERIAL_MSR_DCD;     /* MS_RLSD_ON */
    }
    if (serialState & CDC_SERIAL_STATE_DSR) {
        newModemStatus |= SERIAL_MSR_DSR;     /* MS_DSR_ON */
    }
    if (serialState & CDC_SERIAL_STATE_RING) {
        newModemStatus |= SERIAL_MSR_RI;      /* MS_RING_ON */
    }

    /* Detect changes for event notification */
    {
        ULONG changed = devCtx->ModemStatus ^ newModemStatus;
        if (changed & SERIAL_MSR_DCD) {
            events |= SERIAL_EV_RLSD;
        }
        if (changed & SERIAL_MSR_DSR) {
            events |= SERIAL_EV_DSR;
        }
        if (changed & SERIAL_MSR_RI) {
            events |= SERIAL_EV_RING;
        }
    }

    /* Track error conditions */
    if (serialState & CDC_SERIAL_STATE_FRAMING) {
        devCtx->PerfStats.FrameErrorCount++;
        events |= SERIAL_EV_ERR;
    }
    if (serialState & CDC_SERIAL_STATE_PARITY) {
        devCtx->PerfStats.ParityErrorCount++;
        events |= SERIAL_EV_ERR;
    }
    if (serialState & CDC_SERIAL_STATE_OVERRUN) {
        devCtx->PerfStats.SerialOverrunErrorCount++;
        events |= SERIAL_EV_ERR;
    }
    if (serialState & CDC_SERIAL_STATE_BREAK) {
        events |= SERIAL_EV_BREAK;
    }

    /* Update stored modem status */
    devCtx->ModemStatus = newModemStatus;

    /* Signal any matching wait-on-mask events */
    if (events != 0) {
        SerialCompleteWaitOnMask(devCtx, events);
    }
}

/* =========================================================================
 *  EvtUsbInterruptReadersFailed — error handler for Interrupt IN reader
 *
 *  Returning TRUE tells KMDF to reset the pipe and restart.
 * ========================================================================= */
BOOLEAN
EvtUsbInterruptReadersFailed(
    _In_ WDFUSBPIPE    Pipe,
    _In_ NTSTATUS      Status,
    _In_ USBD_STATUS   UsbdStatus
    )
{
    UNREFERENCED_PARAMETER(Pipe);
    UNREFERENCED_PARAMETER(Status);
    UNREFERENCED_PARAMETER(UsbdStatus);

    return TRUE;
}

/* =========================================================================
 *  EvtWriteRequestComplete — completion routine for bulk OUT writes
 *
 *  Updates performance stats and checks whether a zero-length packet
 *  needs to be sent (when the transfer size is an exact multiple of
 *  the endpoint's max packet size).
 *
 *  Equivalent to the original Data_OutTransComplete (section 10).
 * ========================================================================= */
VOID
EvtWriteRequestComplete(
    _In_ WDFREQUEST                     Request,
    _In_ WDFIOTARGET                    Target,
    _In_ PWDF_REQUEST_COMPLETION_PARAMS Params,
    _In_ WDFCONTEXT                     Context
    )
{
    PDEVICE_CONTEXT devCtx = (PDEVICE_CONTEXT)Context;
    NTSTATUS        status;
    size_t          bytesWritten;

    UNREFERENCED_PARAMETER(Target);

    status       = Params->IoStatus.Status;
    bytesWritten = Params->IoStatus.Information;

    InterlockedDecrement(&devCtx->OutstandingWrites);

    if (NT_SUCCESS(status)) {
        devCtx->PerfStats.TransmittedCount += (ULONG)bytesWritten;

        /*
         * Check for ZLP requirement: if the transfer was an exact
         * multiple of MaxPacketSize, the device may not recognise
         * the end of the transfer. Queue a ZLP work item.
         */
        if (bytesWritten > 0 &&
            devCtx->BulkOutMaxPacket > 0 &&
            (bytesWritten % devCtx->BulkOutMaxPacket) == 0) {
            WdfWorkItemEnqueue(devCtx->ZlpWorkItem);
        }
    }

    WdfRequestCompleteWithInformation(Request, status, bytesWritten);
}

/* =========================================================================
 *  EvtZlpWorkItem — send a zero-length packet on the Bulk OUT pipe
 *
 *  This is queued as a work item from EvtWriteRequestComplete when a
 *  write transfer is an exact multiple of MaxPacketSize.
 * ========================================================================= */
VOID
EvtZlpWorkItem(
    _In_ WDFWORKITEM WorkItem
    )
{
    WDFDEVICE           device = WdfWorkItemGetParentObject(WorkItem);
    PDEVICE_CONTEXT     devCtx = GetDeviceContext(device);
    WDF_MEMORY_DESCRIPTOR memDesc;
    UCHAR               dummy = 0;
    ULONG               bytesTransferred = 0;

    if (devCtx->BulkOutPipe == NULL) {
        return;
    }

    /*
     * Send a zero-length write synchronously.
     * We initialise a 1-byte descriptor so the USB stack always
     * receives a valid buffer, even though no data is transferred.
     */
    WDF_MEMORY_DESCRIPTOR_INIT_BUFFER(&memDesc, &dummy, sizeof(dummy));

    (VOID)WdfUsbTargetPipeWriteSynchronously(
        devCtx->BulkOutPipe,
        WDF_NO_HANDLE,
        NULL,       /* no send options */
        &memDesc,
        &bytesTransferred
        );
}
