/*
 * wmi.c
 *
 * WMI (Windows Management Instrumentation) registration for the serial port.
 * Registers the standard MSSerial_CommInfo WMI data block so that Windows
 * can query serial port information via WMI.
 *
 * Equivalent to the original WMI registration in the proprietary driver.
 *
 * SPDX-License-Identifier: MIT
 */

#include "mtk_usb2ser.h"

#ifdef ALLOC_PRAGMA
#pragma alloc_text(PAGE, WmiRegistration)
#pragma alloc_text(PAGE, EvtWmiInstanceQueryInstance)
#endif

/*
 * WMI GUID for MSSerial_CommInfo — standard serial port WMI class.
 * {EDB16A62-B16C-11D1-BD98-00A0C906BE2D}
 */
DEFINE_GUID(SERIAL_PORT_WMI_COMM_GUID,
    0xEDB16A62, 0xB16C, 0x11D1,
    0xBD, 0x98, 0x00, 0xA0, 0xC9, 0x06, 0xBE, 0x2D);

/* =========================================================================
 *  EvtWmiInstanceQueryInstance — handle WMI queries for serial port info
 * ========================================================================= */
static
NTSTATUS
EvtWmiInstanceQueryInstance(
    _In_  WDFWMIINSTANCE  WmiInstance,
    _In_  ULONG           OutBufferSize,
    _In_  PVOID           OutBuffer,
    _Out_ PULONG          BufferUsed
    )
{
    PDEVICE_CONTEXT devCtx;
    WDFDEVICE       device;

    PAGED_CODE();
    UNREFERENCED_PARAMETER(OutBufferSize);

    device = WdfWmiInstanceGetDevice(WmiInstance);
    devCtx = GetDeviceContext(device);

    /*
     * Return minimal serial port info.
     * The output structure is typically MSSerial_CommInfo with fields
     * for baud rate, data bits, etc.
     *
     * For simplicity, return the baud rate as a ULONG if the buffer
     * is large enough.
     */
    if (OutBufferSize < sizeof(ULONG)) {
        *BufferUsed = sizeof(ULONG);
        return STATUS_BUFFER_TOO_SMALL;
    }

    *(PULONG)OutBuffer = devCtx->LineCoding.dwDTERate;
    *BufferUsed = sizeof(ULONG);

    return STATUS_SUCCESS;
}

/* =========================================================================
 *  WmiRegistration — register WMI data provider for the serial port
 *
 *  Creates a WMI instance for the MSSerial_CommInfo class.
 *  This allows tools like wmic and PowerShell to query serial port state.
 * ========================================================================= */
NTSTATUS
WmiRegistration(
    _In_ WDFDEVICE Device
    )
{
    NTSTATUS                    status;
    WDF_WMI_PROVIDER_CONFIG     providerConfig;
    WDF_WMI_INSTANCE_CONFIG     instanceConfig;
    WDFWMIINSTANCE              wmiInstance;

    PAGED_CODE();

    /* Configure the WMI provider for the serial comm GUID */
    WDF_WMI_PROVIDER_CONFIG_INIT(&providerConfig,
                                  &SERIAL_PORT_WMI_COMM_GUID);
    providerConfig.MinInstanceBufferSize = sizeof(ULONG);

    /* Configure the WMI instance */
    WDF_WMI_INSTANCE_CONFIG_INIT_PROVIDER_CONFIG(
        &instanceConfig,
        &providerConfig
        );
    instanceConfig.Register           = TRUE;
    instanceConfig.EvtWmiInstanceQueryInstance = EvtWmiInstanceQueryInstance;

    status = WdfWmiInstanceCreate(
        Device,
        &instanceConfig,
        WDF_NO_OBJECT_ATTRIBUTES,
        &wmiInstance
        );

    return status;
}
