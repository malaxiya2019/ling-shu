{{- define "lingshu.name" -}}
{{- default .Chart.Name .Values.nameOverride | trunc 63 | trimSuffix "-" }}
{{- end }}

{{- define "lingshu.fullname" -}}
{{- if .Values.fullnameOverride }}
{{- .Values.fullnameOverride | trunc 63 | trimSuffix "-" }}
{{- else }}
{{- $name := default .Chart.Name .Values.nameOverride }}
{{- if contains $name .Release.Name }}
{{- .Release.Name | trunc 63 | trimSuffix "-" }}
{{- else }}
{{- printf "%s-%s" .Release.Name $name | trunc 63 | trimSuffix "-" }}
{{- end }}
{{- end }}
{{- end }}

{{- define "lingshu.labels" -}}
helm.sh/chart: {{ include "lingshu.name" . }}-{{ .Chart.Version }}
app.kubernetes.io/name: {{ include "lingshu.name" . }}
app.kubernetes.io/instance: {{ .Release.Name }}
app.kubernetes.io/version: {{ .Chart.AppVersion }}
app.kubernetes.io/managed-by: {{ .Release.Service }}
{{- end }}

{{- define "lingshu.serverSelectorLabels" -}}
app.kubernetes.io/name: {{ include "lingshu.name" . }}-server
app.kubernetes.io/instance: {{ .Release.Name }}
{{- end }}

{{- define "lingshu.workerSelectorLabels" -}}
app.kubernetes.io/name: {{ include "lingshu.name" . }}-worker
app.kubernetes.io/instance: {{ .Release.Name }}
{{- end }}
