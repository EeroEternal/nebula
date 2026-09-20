import { ModelAbility } from '@/constants/modelData';

export enum CodeType {
  Python = 'Python',
  TypeScript = 'TypeScript',
  Java = 'Java',
  Go = 'Go',
  Shell = 'Shell',
}
export const tabsForApi = Object.values(CodeType);

interface ApiField {
  key: string;
  type?: 'file';
  value?: unknown;
  required?: boolean;
  comment?: string;
  fileName?: string;
}

export interface ApiSchema {
  name: ModelAbility;
  path: string;
  method: 'POST';
  /** json 为 application/json、 form为 multipart/form-data */
  contentType: 'json' | 'form';
  fields: ApiField[];
}
// 单行注释
function getComment(f: ApiField) {
  if (f.required) return '';
  if (f.comment) return `Optional(${f.comment})`;
  return 'Optional';
}
// java, go 不能在 JSON 内写注释，在JSON上新开一行列出非必填字段
// Optional【a、b】
function getOptionalFields(fields: ApiSchema['fields']) {
  const optionalFields = fields.filter((item) => !item.required);
  if (!optionalFields.length) return '';
  return `// Optional【${optionalFields
    .map((item) => (item.comment ? `${item.key}(${item.comment})` : item.key))
    .join(', ')}】`;
}
function formatObjectValue(value: unknown, lang: CodeType, indent: number = 2) {
  let json = JSON.stringify(value, null, 2);

  if (lang === CodeType.Python) {
    json = json.replace(/true/g, 'True').replace(/false/g, 'False');
  }

  const indentStr = ' '.repeat(indent);

  return json
    .split('\n')
    .map((line, index) => {
      if (index === 0) return line; // 第一行不动
      return indentStr + line; // 后续行补缩进
    })
    .join('\n');
}
export type ApiFillIds = { modelUid?: string; replicaId?: string };

function quoteApiStr(value: string) {
  return `"${String(value).replace(/\\/g, '\\\\').replace(/"/g, '\\"')}"`;
}

function getValue(f: ApiField, lang: CodeType, indent: number = 2, fill?: ApiFillIds) {
  if (f.key === 'model') return quoteApiStr(fill?.modelUid || '{MODEL_UID}');
  if (f.key === 'replica_id') return quoteApiStr(fill?.replicaId || '{REPLICA_ID}');
  if (f.key === 'kwargs') {
    const kwargs = f.value ?? { key: 'value' };
    if (lang === CodeType.Python) {
      // Python 直接传对象给 json.dumps
      return `json.dumps(${JSON.stringify(kwargs)})`;
    }
    // 其他语言 → 需要“字符串里的 JSON”
    return JSON.stringify(JSON.stringify(kwargs));
  }
  if (typeof f.value === 'object' && f.value !== null) {
    return formatObjectValue(
      f.value,
      lang === CodeType.Python ? CodeType.Python : CodeType.TypeScript,
      indent,
    );
  }

  if (typeof f.value === 'boolean') {
    if (lang === CodeType.Python) return f.value ? 'True' : 'False';
    return f.value ? 'true' : 'false';
  }

  if (typeof f.value === 'number') return String(f.value);

  return `"${f.value ?? ''}"`;
}

export function generateTS(schema: ApiSchema, fill?: ApiFillIds) {
  const url = `${window.DOMAIN_API}${schema.path}`;

  if (schema.contentType === 'json') {
    return `
const payload = {
${schema.fields
  .filter((f) => f.type !== 'file')
  .map((f) => {
    const comment = getComment(f);
    const value = getValue(f, CodeType.TypeScript, 2, fill);

    return `  "${f.key}": ${value}, ${comment && `// ${comment}`}`;
  })
  .join('\n')}
};

fetch("${url}", {
  "method": "POST",
  "headers": {
    "Accept": "application/json",
    "Content-Type": "application/json",
    "Authorization": "bearer {API_KEY}"
  },
  "body": JSON.stringify(payload)
});
`;
  }

  return `
const formData = new FormData();
${schema.fields
  .map((f) => {
    const comment = getComment(f);

    if (f.type === 'file') {
      return `formData.append("${f.key}", originFileObj); ${comment && `// ${comment}`}`;
    }
    return `formData.append("${f.key}", ${getValue(f, CodeType.TypeScript, 2, fill)}); ${
      comment && `// ${comment}`
    }`;
  })
  .join('\n')}

fetch("${url}", {
  method: "POST",
  headers: {
    "Accept": "application/json",
    "Authorization": "bearer {API_KEY}"
  },
  body: formData
});
`;
}
export function generatePython(schema: ApiSchema, fill?: ApiFillIds) {
  const url = `${window.DOMAIN_API}${schema.path}`;

  if (schema.contentType === 'json') {
    return `
import requests
${schema.fields.filter((item) => item.key === 'kwargs').length ? 'import json\n' : ''}
url = "${url}"

data = {
${schema.fields
  .filter((f) => f.type !== 'file')
  .map((f) => {
    const comment = getComment(f);
    return `  "${f.key}": ${getValue(f, CodeType.Python, 2, fill)}, ${comment && `# ${comment}`}`;
  })
  .join('\n')}
}

headers = {
  "Accept": "application/json",
  "Content-Type": "application/json",
  "Authorization": "bearer {API_KEY}"
}

requests.post(url, headers=headers, json=data)
`;
  }

  return `
import requests
import json

url = "${url}"

files = {
${schema.fields
  .filter((f) => f.type === 'file')
  .map((f) => `  "${f.key}": open("/path/to/${f?.fileName || 'file'}", "rb"),`)
  .join('\n')}
}

data = {
${schema.fields
  .filter((f) => f.type !== 'file')
  .map((f) => {
    const comment = getComment(f);
    return `  "${f.key}": ${getValue(f, CodeType.Python, 2, fill)}, ${comment && `# ${comment}`}`;
  })
  .join('\n')}
}

headers = {
  "Accept": "application/json",
  "Authorization": "bearer {API_KEY}"
}

requests.post(url, headers=headers, data=data, files=files)
`;
}
export function generateGo(schema: ApiSchema, fill?: ApiFillIds) {
  const url = `${window.DOMAIN_API}${schema.path}`;

  if (schema.contentType === 'json') {
    return `
package main

import (
  "bytes"
  "net/http"
)

func main() {
  ${getOptionalFields(schema.fields)}
  jsonData := []byte(\`{
${schema.fields
  .filter((f) => f.type !== 'file')
  .map((f) => {
    return `    "${f.key}": ${getValue(f, CodeType.Go, 4, fill)}`;
  })
  .join(',\n')}
  }\`)

  req, _ := http.NewRequest("POST", "${url}", bytes.NewBuffer(jsonData))
  req.Header.Set("Content-Type", "application/json")
  req.Header.Set("Accept", "application/json")
  req.Header.Set("Authorization", "bearer {API_KEY}")

  http.DefaultClient.Do(req)
}
`;
  }

  return `
  package main
  
  import (
    "bytes"
    "mime/multipart"
    "net/http"
    "os"
  )
  
  func main() {
    var body bytes.Buffer
    writer := multipart.NewWriter(&body)
  
  ${schema.fields
    .map((f, idx) => {
      const comment = getComment(f);
      const indent: string = idx === 0 ? '' : '  ';
      if (f.type === 'file') {
        const fileName = f.fileName || 'file';
        return `  ${indent}file, _ := os.Open("/path/to/${fileName}")
    defer file.Close()
    part, _ := writer.CreateFormFile("${f.key}", "${fileName}")
    io.Copy(part, file)${comment ? ` // ${comment}` : ''}`;
      }
      return `  ${indent}writer.WriteField("${f.key}", ${getValue(f, CodeType.Go, 2, fill)})${
        comment ? ` // ${comment}` : ''
      }`;
    })
    .join('\n')}
  
    writer.Close()
  
    req, _ := http.NewRequest("POST", "${url}", &body)
    req.Header.Set("Content-Type", writer.FormDataContentType())
    req.Header.Set("Accept", "application/json")
    req.Header.Set("Authorization", "bearer {API_KEY}")
  
    http.DefaultClient.Do(req)
  }
  `;
}
export function generateJava(schema: ApiSchema, fill?: ApiFillIds) {
  const url = `${window.DOMAIN_API}${schema.path}`;

  if (schema.contentType === 'json') {
    return `
import okhttp3.*;

public class Main {
  public static void main(String[] args) throws Exception {
    OkHttpClient client = new OkHttpClient();
    ${getOptionalFields(schema.fields)}
    String json = """{
    ${schema.fields
      .filter((f) => f.type !== 'file')
      .map((f, idx) => {
        return `  ${idx !== 0 ? '    ' : ''}"${f.key}": ${getValue(f, CodeType.Java, 6, fill)}`;
      })
      .join(',\n')}
    }""";

    Request request = new Request.Builder()
      .url("${url}")
      .post(RequestBody.create(json, MediaType.parse("application/json")))
      .addHeader("Accept", "application/json")
      .addHeader("Content-Type", "application/json")
      .addHeader("Authorization", "bearer {API_KEY}")
      .build();

    Response response = client.newCall(request).execute();
    System.out.println(response.body().string());
  }
}
`;
  }
  return `
import okhttp3.*;
import java.io.File;

public class Main {
  public static void main(String[] args) throws Exception {
    OkHttpClient client = new OkHttpClient();

    MultipartBody.Builder builder = new MultipartBody.Builder()
      .setType(MultipartBody.FORM);

${schema.fields
  .map((f) => {
    const comment = getComment(f);

    if (f.type === 'file') {
      const fileName = f.fileName || 'file';
      return `    builder.addFormDataPart(
      "${f.key}",
      "${fileName}",
      RequestBody.create(
        new File("/path/to/${fileName}"),
        MediaType.parse("application/octet-stream")
      )
    );${comment ? ` // ${comment}` : ''}`;
    }

    return `    builder.addFormDataPart("${f.key}", ${getValue(f, CodeType.Java, 2, fill)});${
      comment ? ` // ${comment}` : ''
    }`;
  })
  .join('\n')}

    Request request = new Request.Builder()
      .url("${url}")
      .post(builder.build())
      .addHeader("Accept", "application/json")
      .addHeader("Authorization", "bearer {API_KEY}")
      .build();

    Response response = client.newCall(request).execute();
    System.out.println(response.body().string());
  }
}
`;
}
export function generateShell(schema: ApiSchema, fill?: ApiFillIds) {
  const url = `${window.DOMAIN_API}${schema.path}`;

  if (schema.contentType === 'json') {
    return `
curl --request POST \\
  --url ${url} \\
  -H 'Accept: application/json' \\
  -H 'Content-Type: application/json' \\
  -H 'Authorization: bearer {API_KEY}' \\
  --data-raw '{
${schema.fields
  .filter((f) => f.required)
  .map((f) => {
    return `    "${f.key}": ${getValue(f, CodeType.Shell, 4, fill)}`;
  })
  .join(',\n')}
  }'
`;
  }

  return `
curl --request POST \\
  --url ${url} \\
  -H 'Accept: application/json' \\
  -H 'Authorization: bearer {API_KEY}' \\
${schema.fields
  .filter((f) => f.required)
  .map((f) => {
    if (f.type === 'file') {
      return `  -F '${f.key}=@/path/to/${f?.fileName || 'file'}' \\`;
    }
    return `  -F '${f.key}=${getValue(f, CodeType.Shell, 2, fill).replace(/"/g, '')}' \\`;
  })
  .join('\n')}
`;
}
