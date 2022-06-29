import { loadPackageDefinition, credentials, Metadata } from "@grpc/grpc-js"
import { createHash } from "crypto";
import fs from "fs"
import { loadSync } from "@grpc/proto-loader";
import fsPath from "path";

const PROTO_PATH = fsPath.join(__dirname, '../proto/api.proto'),
	SERVER_URL = "192.168.0.248:50051",
	PUBLIC_KEY = "023887a11113c0c72d1f887794490e70ad0f0f7cf81ab43de2998cbdab5b7bfd5a",
	PRIVATE_KEY = "4b3bee129b6f2a9418d1a617803913e3fee922643c628bc8fb48e0b189d104de"

const packageDefinition = loadSync(
	PROTO_PATH,
	{
		keepCase: true,
		longs: String,
		enums: String,
		defaults: true,
		oneofs: true,
	}
);

const apiProto = loadPackageDefinition(packageDefinition).api as any;

const client = new apiProto.Service(SERVER_URL, credentials.createInsecure());

const meta = new Metadata();
meta.add('public_key', PUBLIC_KEY);

const clearDir = async (dir: string) => new Promise((res, rej) => {
	const path = fsPath.join(__dirname, dir)

	fs.rm(path, { recursive: true, force: true }, (err) => {
		if (err) rej(err);
		res(null)
	})
})

const downloadFile = async () => {
	const sig = "3044022078cfbc1a9d1e63cf94ef52151f6a1e95101b27bde1b9abbaac1163ebdf363e5002204a17153ca42c8170dc0371bae8d38934cc21ae7f942fc2e6f02c7678affc0736";

	const call = client.Get(
		{
			location: "e_somelocation/folder/e_3044022059561fd42dcd9640e8b032b20f7b4575f895ab1e9d9fe479718c02026bee6e69022033596df910d8881949af6dddc50d63e8948c688cd74e91293ac74f8c3d9f891a/",
			sig,
			download: true
		},
		meta
	)

	await clearDir("../download")
	call.on("data", (res: any) => {
		 console.log(res[res.download_response])
		if (res.download_response === "file") {
			const folderPath = fsPath.join("./download", fsPath.dirname(res[res.download_response].name));
			if (!fs.existsSync(folderPath)) fs.mkdirSync(folderPath, { recursive: true });

			const fullPath = fsPath.join("./download", res[res.download_response].name)
			fs.appendFileSync(fullPath, res[res.download_response].content);
		}
	})

	call.on("end", () => {
		console.log("end")
	})
}

type Type = "file" | "directory"
type Children = { type: Type, name: string, entry: null, cids: string[], size: number }[]

interface MetaData {
	name: string
	children: Children
}

const MAX_CHUNK_SIZE = 1024 * 256

const getMetaDataForDir = (path: string): null | MetaData => {
	if (!fs.existsSync(path)) return null

	const metaData: MetaData = {
		name: fsPath.basename(path),
		children: []
	}

	const getChildren = (path: string, basePath: string = ""): Children => {
		let children: Children = []

		fs.readdirSync(path).forEach(entry => {
			let entryPath = fsPath.join(path, entry)

			if (fs.lstatSync(entryPath).isDirectory())
				children = [...children, ...getChildren(entryPath, fsPath.join(basePath, entry))]
			else {
				let data = fs.readFileSync(fsPath.join(path, entry))

				let hasher = createHash("sha256")
				hasher.update(data)

				let cid = hasher.digest("hex");

				children.push({
					name: fsPath.join(basePath, entry),
					type: "file",
					entry: null,
					cids: [],
					size: data.length
				})
			}
		})

		return children
	}

	metaData.children = getChildren(path)

	return metaData
}

const addCidsToChildren = async (children: Children, path: string): Promise<Children> => {
	for (const { name, type, cids } of children) {
		if (type === "file") {
			const filePath = fsPath.join(path, name),
				stream = fs.createReadStream(fsPath.join(__dirname, filePath), { highWaterMark: MAX_CHUNK_SIZE })

			await new Promise((res, rej) => {
				stream.on("data", (chunk) => {
					let hasher = createHash("sha256")
					hasher.update(chunk)

					let cid = hasher.digest("hex");

					cids.push(cid)
				})
				stream.on("end", res)
				stream.on("error", (e) => console.error(e))
			})
		}
	}

	return children
}

const createReadStreams = (paths: { path: string }[]) => (paths.map(
	({ path }) => ({ path, stream: fs.createReadStream(fsPath.join(__dirname, path), { highWaterMark: MAX_CHUNK_SIZE }) })
))

const uploadDirectory = async (path: string) => {
	let isPublic = true,
		read_users: string[] = []

	const metaData = getMetaDataForDir(fsPath.join(__dirname, path))
	if (!metaData) return

	metaData.children = await addCidsToChildren(metaData.children, path)

	let entry = {
		owner: PUBLIC_KEY,
		public: isPublic,
		read_users,
		children: metaData?.children,
		name: metaData.name
	}
	console.log(entry.children)

	let request = {
		metadata: {
			signature: "3044022059561fd42dcd9640e8b032b20f7b4575f895ab1e9d9fe479718c02026bee6e69022033596df910d8881949af6dddc50d63e8948c688cd74e91293ac74f8c3d9f891a",
			entry
		}
	}

	let call = client.Put(meta, (err: any, res: { key: string }) => {
		console.log(err, res)
	})

	await call.write(request)

	for (let { stream } of createReadStreams(metaData.children.map(({ name }) => ({ path: fsPath.join(path, name) })))) {
		await new Promise((res, rej) => {
			stream.on("data", (chunk) => {
				console.log(chunk.length)
				let hasher = createHash("sha256")
				hasher.update(chunk)

				let cid = hasher.digest("hex");
				call.write({ file: { content: chunk, cid } })
			})
			stream.on("end", res)
			stream.on("error", (e) => console.log("TEST", e.message))
		})
	}

	await call.end()
}

//uploadDirectory("../test/Hello")
downloadFile()
