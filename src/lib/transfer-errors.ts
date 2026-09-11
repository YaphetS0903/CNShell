export const friendlyTransferError=(error:string)=>{
  const normalized=error.toLocaleLowerCase();
  if(/session|连接.*失效|未找到连接/.test(normalized))return"连接已失效，请重新连接后重试。";
  if(/permission denied|sftp\s*\(?3\)?|not permitted|权限/.test(normalized))return"目标目录不可写，请检查权限或更换路径后重试。";
  if(/no such file|not found|不存在/.test(normalized))return"源文件或目标路径不存在，请确认路径后重试。";
  if(/no space|disk full|空间不足/.test(normalized))return"目标磁盘空间不足，请释放空间后重试。";
  if(/timed?\s*out|timeout|超时/.test(normalized))return"连接超时，请检查网络后重试。";
  return"传输失败，请重试或展开技术详情查看原因。";
};
